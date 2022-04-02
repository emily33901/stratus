mod hls_source;
mod mp3;

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU16, AtomicUsize},
        Arc,
    },
    thread, time,
};

use async_trait::async_trait;
use eyre::Result;
use log::{info, warn};
use m3u8_rs::playlist::MediaPlaylist;
use rodio::{Decoder, Source};
use tokio::{select, sync::mpsc, sync::oneshot, sync::watch, sync::Mutex};

use crate::mp3::HlsDecoder;

#[async_trait]
pub trait Downloader: Send + Sync {
    async fn download(&self, url: &str) -> Result<Vec<u8>>;
}

pub type SongId = i64;

#[derive(Debug)]
pub(crate) struct QueuedSong {
    pub(crate) playlist: MediaPlaylist,
    pub(crate) id: SongId,
}

impl QueuedSong {
    pub(crate) fn new(playlist: MediaPlaylist, id: SongId) -> Self {
        QueuedSong { playlist, id }
    }
}

#[derive(Debug)]
enum PlayerControl {
    Pause,
    Resume,
    SkipAll,
    SkipOne,
    Volume(f32),
    Seek(usize),
    Queue(QueuedSong),
}

pub struct HlsPlayer {
    control: mpsc::Sender<PlayerControl>,
    pos: Arc<AtomicUsize>,
    total: Arc<parking_lot::Mutex<f32>>,
    cur_song: watch::Receiver<Option<SongId>>,
    queued_song: watch::Receiver<VecDeque<SongId>>,
}

impl HlsPlayer {
    pub fn new(downloader: Arc<dyn Downloader>) -> Self {
        let (control_tx, control_rx) = mpsc::channel(10);
        let loop_control_tx = control_tx.clone();

        let pos = Arc::new(AtomicUsize::new(0));
        let loop_pos = pos.clone();

        let total: Arc<parking_lot::Mutex<f32>> = Default::default();
        let loop_total = total.clone();

        let (cur_song_tx, cur_song_rx) = watch::channel(None);
        let (queued_song_tx, queued_song_rx) = watch::channel(VecDeque::new());

        let cur_song_rx2 = cur_song_rx.clone();
        thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(player_control(
                    control_rx,
                    downloader,
                    loop_pos,
                    loop_total,
                    loop_control_tx,
                    cur_song_tx,
                    cur_song_rx2,
                    queued_song_tx,
                ));
        });

        Self {
            pos,
            total,
            control: control_tx,
            cur_song: cur_song_rx,
            queued_song: queued_song_rx,
        }
    }

    pub async fn queue(&self, playlist: &str, id: SongId) -> Result<()> {
        let bytes = playlist.as_bytes().to_vec();
        let playlist = m3u8_rs::parse_media_playlist_res(&bytes).unwrap();

        Ok(self
            .control
            .send(PlayerControl::Queue(QueuedSong::new(playlist, id)))
            .await?)
    }

    pub async fn resume(&self) -> Result<()> {
        info!("Resuming playback");
        Ok(self.control.send(PlayerControl::Resume).await?)
    }

    pub async fn pause(&self) -> Result<()> {
        info!("Pausing playback");
        Ok(self.control.send(PlayerControl::Pause).await?)
    }

    pub async fn stop(&self) -> Result<()> {
        info!("Stopping playback");
        // self.control.send(PlayerControl::).await?;
        Ok(())
    }

    pub async fn skip(&self) -> Result<()> {
        info!("Skipping track");
        Ok(self.control.send(PlayerControl::SkipOne).await?)
    }

    pub fn position(&self) -> usize {
        self.pos.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn total(&self) -> f32 {
        *self.total.lock()
    }

    pub fn queued_watch(&self) -> watch::Receiver<VecDeque<SongId>> {
        self.queued_song.clone()
    }
}

async fn player_control(
    mut control_rx: mpsc::Receiver<PlayerControl>,
    downloader: Arc<dyn Downloader>,
    pos: Arc<AtomicUsize>,
    total: Arc<parking_lot::Mutex<f32>>,
    loop_control_tx: mpsc::Sender<PlayerControl>,
    cur_song_tx: watch::Sender<Option<SongId>>,
    cur_song_rx: watch::Receiver<Option<SongId>>,
    queued_song_tx: watch::Sender<VecDeque<SongId>>,
) {
    let (sink, stream, handle) = {
        let (stream, handle) = rodio::OutputStream::try_default().unwrap();
        let sink = Mutex::new(rodio::Sink::try_new(&handle).unwrap());
        (sink, Mutex::new(stream), Mutex::new(handle))
    };
    let reset_sink = || async {
        let (new_stream, new_handle) = rodio::OutputStream::try_default().unwrap();
        let new_sink = rodio::Sink::try_new(&new_handle).unwrap();
        *sink.lock().await = new_sink;
        *stream.lock().await = new_stream;
        *handle.lock().await = new_handle;
    };

    // TODO(emily): Queue should probably just be a shared bit of memory so that it doesn't
    // have to be copied around everwhere
    let mut queue = VecDeque::<QueuedSong>::new();

    let (finished_signal_tx, mut finished_signal_rx) = mpsc::channel::<()>(1);

    loop {
        select! {
            Some(control) = control_rx.recv() => {
                match control {
                    PlayerControl::Pause => sink.lock().await.pause(),
                    PlayerControl::Resume => sink.lock().await.play(),
                    PlayerControl::SkipAll => reset_sink().await,
                    PlayerControl::SkipOne => {
                        if let Some(queued_track) = queue.pop_front() {
                            // Resend the updated queue
                            queued_song_tx
                                .send(queue.iter().map(|x| x.id).collect())
                                .unwrap();

                            // Calculate the total length of the track
                            *total.lock() = queued_track
                                .playlist
                                .segments
                                .iter()
                                .map(|x| x.duration)
                                .sum::<f32>();

                            let QueuedSong { id, playlist } = queued_track;

                            // Reset sink
                            reset_sink().await;
                            // Tell everyone that we are playing a new track
                            cur_song_tx.send(Some(id)).unwrap();

                            let cur_song_rx = cur_song_rx.clone();
                            let downloader = downloader.clone();
                            let chunk_rx = download_hls_segments(playlist, downloader, cur_song_rx).await;

                            let decoder = HlsDecoder::new(chunk_rx, &finished_signal_tx).await.unwrap();
                            let pos = pos.clone();
                            let periodic =
                                decoder.periodic_access(time::Duration::from_millis(100), move |decoder| {
                                    pos.store(decoder.samples(), std::sync::atomic::Ordering::Relaxed);
                                });
                            let sink = sink.lock().await;
                            sink.append(periodic);
                            sink.play();
                        } else {
                            // Nothing in queue so reset sink and inform everyone
                            reset_sink().await;
                            cur_song_tx.send(None).unwrap();
                        }
                    }
                    PlayerControl::Queue(playlist) => {
                        info!("Queuing track");
                        queue.push_back(playlist);
                        queued_song_tx
                            .send(queue.iter().map(|x| x.id).collect())
                            .unwrap();
                        if queue.len() == 1 && sink.lock().await.empty() {
                            loop_control_tx.send(PlayerControl::SkipOne).await.unwrap();
                        }
                    }
                    PlayerControl::Volume(_) => todo!(),
                    PlayerControl::Seek(_) => todo!(),
                }
            }
            _ = finished_signal_rx.recv() => {
                loop_control_tx.send(PlayerControl::SkipOne).await.unwrap();
            }
        }
    }

    warn!("player_control going down!");
}

async fn download_hls_segments(
    playlist: MediaPlaylist,
    downloader: Arc<dyn Downloader>,
    mut cur_song_rx: watch::Receiver<Option<SongId>>,
) -> mpsc::Receiver<Vec<u8>> {
    // Acknowledge current track id
    cur_song_rx.changed().await.unwrap();

    let (tx_chunk, rx_chunk) = mpsc::channel(10);

    tokio::spawn(async move {
        for (i, s) in playlist.segments.iter().enumerate() {
            select! {
                downloaded = downloader.download(&s.uri) => {
                    if let Ok(downloaded) = downloaded {
                        info!("downloaded {i}");

                        match tx_chunk.send(downloaded).await {
                            Ok(_) => {}
                            Err(err) => {
                                warn!("rx died - Stopping download");
                                break;
                            }
                        }
                        // r2.write().extend(&downloaded);
                    } else {
                        warn!("Failed to download HLS Segment {} {:?}", i, downloaded);
                    }
                }
                // Ok(_) = cur_song_rx.changed() => {
                //     warn!("Track changed, stopping download");
                //     break;
                // }
            }
        }
    });

    return rx_chunk;
}
