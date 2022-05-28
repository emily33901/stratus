mod hls_source;
mod mp3;

use std::{
    collections::{HashMap, VecDeque},
    error::Error,
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
    /// Download a chunk of a HLS stream
    async fn download_chunk(&self, url: &str) -> Result<Vec<u8>>;
    /// Download a playlist for a SongId
    async fn playlist(&self, id: SongId) -> Result<String>;
    /// Downloads a playlist for a SongId and parses it
    async fn media_playlist(&self, id: SongId) -> Result<MediaPlaylist> {
        let playlist = self.playlist(id).await?;
        let bytes = playlist.as_bytes().to_vec();
        Ok(m3u8_rs::parse_media_playlist(&bytes).unwrap().1)
    }
}

pub type SongId = i64;

#[derive(Debug)]
enum PlayerControl {
    Pause,
    Resume,
    SkipAll,
    SkipOne,
    Volume(f32),
    Seek(usize),
    Queue(SongId),
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

        Ok(self.control.send(PlayerControl::Queue(id)).await?)
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

    pub fn cur_song(&self) -> watch::Receiver<Option<SongId>> {
        self.cur_song.clone()
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
    let mut queue = VecDeque::<SongId>::new();

    let (finished_signal_tx, mut finished_signal_rx) = mpsc::channel::<()>(1);

    loop {
        select! {
            Some(control) = control_rx.recv() => {
                match control {
                    PlayerControl::Pause => sink.lock().await.pause(),
                    PlayerControl::Resume => sink.lock().await.play(),
                    PlayerControl::SkipAll => reset_sink().await,
                    PlayerControl::SkipOne => {
                        if let Some(queued_song) = queue.pop_front() {
                            // Resend the updated queue
                            queued_song_tx
                                .send(queue.clone())
                                .unwrap();

                            // Ask for the playlist AOT
                            let playlist = downloader.media_playlist(queued_song).await.unwrap();

                            // Calculate the total length of the track
                            *total.lock() = playlist
                                .segments
                                .iter()
                                .map(|x| x.duration)
                                .sum::<f32>();

                            // Reset sink
                            reset_sink().await;
                            // If we got a finished signal then consume it
                            finished_signal_rx.try_recv()
                                .map(|_| info!("Consumed a finished signal"))
                                .unwrap_or_else(|_| { info!("No finished signal to consume"); ()});
                            // Tell everyone that we are playing a new track
                            cur_song_tx.send(Some(queued_song)).unwrap();

                            let cur_song_rx = cur_song_rx.clone();
                            let downloader = downloader.clone();
                            let chunk_rx = download_hls_segments(playlist, downloader, cur_song_rx).await;

                            match HlsDecoder::new(chunk_rx, &finished_signal_tx).await {
                                Ok(decoder) => {
                                    let pos = pos.clone();
                                    let periodic =
                                        decoder.periodic_access(time::Duration::from_millis(100), move |decoder| {
                                            pos.store(decoder.samples(), std::sync::atomic::Ordering::Relaxed);
                                        });
                                    let sink = sink.lock().await;
                                    sink.append(periodic);
                                    sink.play();
                                }
                                Err(err) => {
                                    warn!("Failed to get first chunks of HlsDecoder {:?}", err);
                                    loop_control_tx.send(PlayerControl::SkipOne).await.unwrap();
                                }
                            }
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
                            .send(queue.clone())
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
                info!("Finished signal");
                loop_control_tx.send(PlayerControl::SkipOne).await.unwrap();
            }
        }
    }
}

async fn download_hls_segments(
    mut playlist: MediaPlaylist,
    downloader: Arc<dyn Downloader>,
    mut cur_song_rx: watch::Receiver<Option<SongId>>,
) -> mpsc::Receiver<Vec<u8>> {
    // Acknowledge current track id
    cur_song_rx.changed().await.unwrap();
    let id = cur_song_rx.borrow().clone().unwrap();

    let (tx_chunk, rx_chunk) = mpsc::channel(10);

    let mut i = 0;

    tokio::spawn(async move {
        while i < playlist.segments.len() {
            match downloader.download_chunk(&playlist.segments[i].uri).await {
                Ok(chunk) => {
                    // We successfully got the ith chunk, lets keep going
                    info!("downloaded {i}");
                    i += 1;

                    match tx_chunk.send(chunk).await {
                        Ok(_) => {}
                        Err(err) => {
                            warn!("rx died ({:?}) - Stopping download", err.source());
                            break;
                        }
                    }
                }
                Err(err) => {
                    warn!("Failed to download HLS Segment {} {:?}", i, err);
                    // NOTE(emily): The playlist we were downloading might have just expired
                    // So we are going to try and get the playlist again...
                    if let Ok(new_playlist) = downloader.media_playlist(id).await {
                        info!("Successfully updated playlist");
                        playlist = new_playlist;
                    } else {
                        warn!("Failed to re-download playlist... No longer downloading");
                        return;
                    }
                }
            }
        }
        // TODO(emily): Send some signal here that the playlist is done.
    });

    return rx_chunk;
}
