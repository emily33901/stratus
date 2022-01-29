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
use hls_source::HlsReader;
use log::{info, warn};
use m3u8_rs::playlist::MediaPlaylist;
use rodio::{Decoder, Source};
use tokio::{select, sync::mpsc, sync::oneshot, sync::watch, sync::Mutex};

use crate::mp3::HlsDecoder;

#[async_trait]
pub trait Downloader: Send + Sync {
    async fn download(&self, url: &str) -> Result<Vec<u8>>;
}

pub type TrackId = u64;

#[derive(Debug)]
pub(crate) struct QueuedTrack {
    pub(crate) playlist: MediaPlaylist,
    pub(crate) id: TrackId,
}

impl QueuedTrack {
    pub(crate) fn new(playlist: MediaPlaylist, id: u64) -> Self {
        QueuedTrack { playlist, id }
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
    Queue(QueuedTrack),
}

pub struct HlsPlayer {
    control: mpsc::Sender<PlayerControl>,
    pos: Arc<AtomicUsize>,
    total: Arc<parking_lot::Mutex<f32>>,
    cur_track: watch::Receiver<Option<TrackId>>,
    queued_track: watch::Receiver<VecDeque<TrackId>>,
}

impl HlsPlayer {
    pub fn new(downloader: Arc<dyn Downloader>) -> Self {
        let (control_tx, control_rx) = mpsc::channel(10);
        let loop_control_tx = control_tx.clone();

        let pos = Arc::new(AtomicUsize::new(0));
        let loop_pos = pos.clone();

        let total: Arc<parking_lot::Mutex<f32>> = Default::default();
        let loop_total = total.clone();

        let (cur_track_tx, cur_track_rx) = watch::channel(None);
        let (queued_track_tx, queued_track_rx) = watch::channel(VecDeque::new());

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
                    cur_track_tx,
                    queued_track_tx,
                ));
        });

        Self {
            pos,
            total,
            control: control_tx,
            cur_track: cur_track_rx,
            queued_track: queued_track_rx,
        }
    }

    pub async fn queue(&self, playlist: &str, id: u64) -> Result<()> {
        let bytes = playlist.as_bytes().to_vec();
        let playlist = m3u8_rs::parse_media_playlist_res(&bytes).unwrap();

        Ok(self
            .control
            .send(PlayerControl::Queue(QueuedTrack::new(playlist, id)))
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

    pub fn queued_watch(&self) -> watch::Receiver<VecDeque<TrackId>> {
        self.queued_track.clone()
    }
}

async fn player_control(
    mut control_rx: mpsc::Receiver<PlayerControl>,
    downloader: Arc<dyn Downloader>,
    pos: Arc<AtomicUsize>,
    total: Arc<parking_lot::Mutex<f32>>,
    loop_control_tx: mpsc::Sender<PlayerControl>,
    cur_track_tx: watch::Sender<Option<TrackId>>,
    queued_track_tx: watch::Sender<VecDeque<TrackId>>,
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
    let mut queue = VecDeque::<QueuedTrack>::new();

    let (finished_signal_tx, mut finished_signal_rx) = mpsc::channel::<()>(1);

    loop {
        let downloader = downloader.clone();
        select! {
            Some(control) = control_rx.recv() => {
                match control {
                    PlayerControl::Pause => sink.lock().await.pause(),
                    PlayerControl::Resume => sink.lock().await.play(),
                    PlayerControl::SkipAll => reset_sink().await,
                    PlayerControl::SkipOne => {
                        if let Some(queued_track) = queue.pop_front() {
                            queued_track_tx
                                .send(queue.iter().map(|x| x.id).collect())
                                .unwrap();

                            *total.lock() = queued_track
                                .playlist
                                .segments
                                .iter()
                                .map(|x| x.duration)
                                .sum::<f32>();

                            let reader = HlsReader::default();
                            let r2 = reader.clone();

                            let (ready_tx, mut ready_rx) = mpsc::channel::<()>(1);

                            // TODO(emily): Need to add some cancel in here to stop this from happening after tracks are
                            // skipped
                            tokio::spawn(async move {
                                for (i, s) in queued_track.playlist.segments.iter().enumerate() {
                                    let downloaded = downloader.download(&s.uri).await.unwrap();
                                    r2.add(&downloaded);

                                    info!("downloaded {i}");

                                    if i == 1 {
                                        ready_tx.send(()).await.unwrap();
                                    }
                                }
                            });
                            ready_rx.recv().await;

                            cur_track_tx.send(Some(queued_track.id)).unwrap();

                            reset_sink().await;
                            let decoder = HlsDecoder::new(reader, &finished_signal_tx).unwrap();
                            let pos = pos.clone();
                            let periodic =
                                decoder.periodic_access(time::Duration::from_millis(100), move |decoder| {
                                    pos.store(decoder.samples(), std::sync::atomic::Ordering::Relaxed);
                                });
                            let sink = sink.lock().await;
                            sink.append(periodic);
                            sink.play();
                        } else {
                            reset_sink().await;
                        }
                    }
                    PlayerControl::Queue(playlist) => {
                        info!("Queuing track");
                        queue.push_back(playlist);
                        queued_track_tx
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
}
