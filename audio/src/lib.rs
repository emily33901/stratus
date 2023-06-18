mod hls_source;
mod mp3;

use std::{collections::VecDeque, error::Error, sync::Arc, thread, time};

use async_trait::async_trait;
use eyre::Result;
use log::{info, warn};
use m3u8_rs::playlist::MediaPlaylist;
use rodio::Source;
use tokio::{
    select,
    sync::watch,
    sync::Mutex,
    sync::{mpsc, MappedMutexGuard},
};

use crate::mp3::HlsDecoder;

#[derive(Default, Debug, Clone, Copy)]
pub enum Playing {
    Playing,
    #[default]
    Paused,
}

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
    QueueMany(Vec<SongId>),
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub playing: Playing,

    pub sample_rate: usize,
    /// Number of samples into the track
    pub pos: usize,
    /// total time in seconds
    pub total: f32,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            playing: Default::default(),
            sample_rate: 44100,
            pos: Default::default(),
            total: Default::default(),
        }
    }
}

pub struct HlsPlayer {
    control: mpsc::Sender<PlayerControl>,
    state_rx: watch::Receiver<PlayerState>,
    cur_song: watch::Receiver<Option<SongId>>,
    queued_song: watch::Receiver<VecDeque<SongId>>,
}

impl HlsPlayer {
    pub fn new(downloader: Arc<dyn Downloader>) -> Self {
        let (control_tx, control_rx) = mpsc::channel(10);
        let loop_control_tx = control_tx.clone();

        // TODO(emily): Make Option
        let (state_tx, state_rx) = watch::channel(PlayerState::default());

        let (cur_song_tx, cur_song_rx) = watch::channel(None);
        let (queued_song_tx, queued_song_rx) = watch::channel(VecDeque::new());

        let cur_song_rx2 = cur_song_rx.clone();

        thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let mut inner = Inner::new(
                        control_rx,
                        downloader,
                        Arc::new(state_tx),
                        loop_control_tx,
                        cur_song_tx,
                        cur_song_rx2,
                        queued_song_tx,
                    );

                    inner.run().await;
                });
        });

        Self {
            control: control_tx,
            cur_song: cur_song_rx,
            queued_song: queued_song_rx,
            state_rx,
        }
    }

    pub async fn queue(&self, id: SongId) -> Result<()> {
        Ok(self.control.send(PlayerControl::Queue(id)).await?)
    }

    pub async fn queue_many(&self, ids: Vec<SongId>) -> Result<()> {
        Ok(self.control.send(PlayerControl::QueueMany(ids)).await?)
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

    pub fn state_rx(&self) -> watch::Receiver<PlayerState> {
        self.state_rx.clone()
    }

    pub fn queued_watch(&self) -> watch::Receiver<VecDeque<SongId>> {
        self.queued_song.clone()
    }

    pub fn cur_song(&self) -> watch::Receiver<Option<SongId>> {
        self.cur_song.clone()
    }
}

struct SinkStream {
    sink: rodio::Sink,
    stream: rodio::OutputStream,
    handle: rodio::OutputStreamHandle,
}

impl SinkStream {
    fn new() -> Self {
        let (new_stream, new_handle) = rodio::OutputStream::try_default().unwrap();
        let new_sink = rodio::Sink::try_new(&new_handle).unwrap();

        Self {
            sink: new_sink,
            stream: new_stream,
            handle: new_handle,
        }
    }

    fn reset(&mut self) {
        let (new_stream, new_handle) = rodio::OutputStream::try_default().unwrap();
        let new_sink = rodio::Sink::try_new(&new_handle).unwrap();
        self.sink = new_sink;
        self.stream = new_stream;
        self.handle = new_handle;
    }
}

struct Inner {
    control_rx: mpsc::Receiver<PlayerControl>,
    downloader: Arc<dyn Downloader>,
    state_tx: Arc<watch::Sender<PlayerState>>,
    loop_control_tx: mpsc::Sender<PlayerControl>,
    cur_song_tx: watch::Sender<Option<SongId>>,
    cur_song_rx: watch::Receiver<Option<SongId>>,
    queued_song_tx: watch::Sender<VecDeque<SongId>>,
    queue: VecDeque<SongId>,
    sink_stream: Mutex<SinkStream>,
    finished_signal_tx: mpsc::Sender<()>,
    finished_signal_rx: Option<mpsc::Receiver<()>>,
}

impl Inner {
    fn new(
        control_rx: mpsc::Receiver<PlayerControl>,
        downloader: Arc<dyn Downloader>,
        state_tx: Arc<watch::Sender<PlayerState>>,
        loop_control_tx: mpsc::Sender<PlayerControl>,
        cur_song_tx: watch::Sender<Option<SongId>>,
        cur_song_rx: watch::Receiver<Option<SongId>>,
        queued_song_tx: watch::Sender<VecDeque<SongId>>,
    ) -> Self {
        let (finished_signal_tx, finished_signal_rx) = mpsc::channel::<()>(1);

        Self {
            control_rx,
            downloader,
            state_tx,
            loop_control_tx,
            cur_song_tx,
            cur_song_rx,
            queued_song_tx,
            queue: VecDeque::new(),
            sink_stream: Mutex::new(SinkStream::new()),
            finished_signal_tx,
            finished_signal_rx: Some(finished_signal_rx),
        }
    }

    async fn run(&mut self) {
        let mut finished_signal_rx = self.finished_signal_rx.take().unwrap();

        loop {
            select! {
                Some(control) = self.control_rx.recv() => {
                    self.handle_control(control, &mut finished_signal_rx).await;
                }
                _ = finished_signal_rx.recv() => {
                    info!("Finished signal");
                    self.loop_control_tx.send(PlayerControl::SkipOne).await.unwrap();
                }
            }
        }
    }

    async fn sink(&self) -> MappedMutexGuard<rodio::Sink> {
        tokio::sync::MutexGuard::map(self.sink_stream.lock().await, |s| &mut s.sink)
    }

    async fn reset_sink(&self) {
        self.sink_stream.lock().await.reset();
    }

    async fn handle_control(
        &mut self,
        control: PlayerControl,
        finished_signal_rx: &mut mpsc::Receiver<()>,
    ) {
        match control {
            PlayerControl::Pause => {
                self.sink().await.pause();
                self.state_tx.send_modify(|state| {
                    state.playing = Playing::Paused;
                });
            }
            PlayerControl::Resume => {
                self.sink().await.play();
                self.state_tx.send_modify(|state| {
                    state.playing = Playing::Playing;
                });
            }
            PlayerControl::SkipAll => self.reset_sink().await,
            PlayerControl::SkipOne => {
                self.skip_one(finished_signal_rx).await;
            }
            PlayerControl::Queue(id) => {
                info!("Queuing track");
                self.queue.push_back(id);
                self.queued_song_tx.send(self.queue.clone()).unwrap();
                if self.queue.len() == 1 && self.sink().await.empty() {
                    let loop_control_tx = self.loop_control_tx.clone();
                    tokio::spawn(async move {
                        loop_control_tx.send(PlayerControl::SkipOne).await.unwrap()
                    });
                }
            }
            PlayerControl::QueueMany(ids) => {
                info!("Queuing many");
                self.queue.extend(ids.iter());
                self.queued_song_tx.send_modify(|queue| queue.extend(ids));
                if self.sink().await.empty() {
                    let loop_control_tx = self.loop_control_tx.clone();
                    tokio::spawn(async move {
                        loop_control_tx.send(PlayerControl::SkipOne).await.unwrap()
                    });
                }
            }
            PlayerControl::Volume(_) => todo!(),
            PlayerControl::Seek(_) => todo!(),
        }
    }

    async fn skip_one(&mut self, finished_signal_rx: &mut mpsc::Receiver<()>) {
        if let Some(queued_song) = self.queue.pop_front() {
            // Resend the updated queue
            self.queued_song_tx.send_modify(|queue| {
                queue.pop_front();
            });

            // Ask for the playlist AOT
            let playlist = self.downloader.media_playlist(queued_song).await.unwrap();

            // Calculate the total length of the track
            let total = playlist.segments.iter().map(|x| x.duration).sum::<f32>();

            // Reset sink
            self.reset_sink().await;
            // If we got a finished signal then consume it
            finished_signal_rx
                .try_recv()
                .map(|_| info!("Consumed a finished signal"))
                .unwrap_or_else(|_| {
                    info!("No finished signal to consume");
                    ()
                });
            // Tell everyone that we are playing a new track
            self.cur_song_tx.send(Some(queued_song)).unwrap();

            let chunk_rx = self.download_hls_segments(playlist).await;

            match HlsDecoder::new(chunk_rx, &self.finished_signal_tx).await {
                Ok(decoder) => {
                    let state_tx = self.state_tx.clone();
                    let periodic =
                        decoder.periodic_access(time::Duration::from_millis(100), move |decoder| {
                            state_tx.send_modify(|state| {
                                state.playing = Playing::Playing;
                                (state.sample_rate, state.pos, state.total) = (
                                    decoder.sample_rate() as usize,
                                    decoder.samples(),
                                    total.clone(),
                                );
                            });
                        });

                    let sink = self.sink().await;
                    sink.append(periodic);
                    sink.play();
                }
                Err(err) => {
                    warn!("Failed to get first chunks of HlsDecoder {:?}", err);
                    self.loop_control_tx
                        .send(PlayerControl::SkipOne)
                        .await
                        .unwrap();
                }
            }
        } else {
            // Nothing in queue so reset sink and inform everyone
            self.reset_sink().await;
            self.cur_song_tx.send(None).unwrap();
        }
    }

    async fn download_hls_segments(
        &mut self,
        mut playlist: MediaPlaylist,
        // downloader: Arc<dyn Downloader>,
        // mut cur_song_rx: watch::Receiver<Option<SongId>>,
    ) -> mpsc::Receiver<Vec<u8>> {
        // Acknowledge current track id
        self.cur_song_rx.changed().await.unwrap();
        let id = self.cur_song_rx.borrow().clone().unwrap();

        // Buffer bound here is how many chunks ahead we download before waiting for them
        // to get played. On average a chunk is ~1 second.
        let (tx_chunk, rx_chunk) = mpsc::channel(10);
        let downloader = self.downloader.clone();

        tokio::spawn(async move {
            let mut i = 0;
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
}
