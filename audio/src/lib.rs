mod hls_source;
mod mp3;

use std::{collections::VecDeque, error::Error, sync::Arc, thread, time};

use async_trait::async_trait;
use eyre::Result;
use log::{info, warn};
use m3u8_rs::playlist::MediaPlaylist;
use rodio::Source;
use tokio::{select, sync::mpsc, sync::watch, sync::Mutex};

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

#[derive(Default, Debug, Clone)]
pub struct PlayerState {
    pub playing: Playing,

    pub sample_rate: usize,
    /// Number of samples into the track
    pub pos: usize,
    /// total time in seconds
    pub total: f32,
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
                .block_on(player_control(
                    control_rx,
                    downloader,
                    state_tx,
                    loop_control_tx,
                    cur_song_tx,
                    cur_song_rx2,
                    queued_song_tx,
                ));
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

async fn player_control(
    mut control_rx: mpsc::Receiver<PlayerControl>,
    downloader: Arc<dyn Downloader>,
    state_tx: watch::Sender<PlayerState>,
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

    let mut queue = VecDeque::<SongId>::new();

    let state_tx = Arc::new(state_tx);

    // Appease compiler by creating an mpsc channel that we use within this loop, and then relay to the
    // outside watch here.
    // I think this shouldnt be needed, but the compiler says otherwise
    // let (position_tx, mut position_rx) = mpsc::channel(1);
    // let (playing_tx, mut playing_rx) = mpsc::channel(1);
    // let _handles = vec![
    //     tokio::spawn(async {
    //         while let Some(position) = position_rx.recv().await {
    //             state_tx.send_modify(|state| {
    //                 (state.sample_rate, state.pos, state.total) = position;
    //             })
    //         }
    //     }),
    //     tokio::spawn(async {
    //         while let Some(playing) = playing_rx.recv().await {
    //             state_tx.send_modify(|state| {
    //                 state.playing = playing;
    //             })
    //         }
    //     }),
    // ];

    let (finished_signal_tx, mut finished_signal_rx) = mpsc::channel::<()>(1);

    loop {
        select! {
            Some(control) = control_rx.recv() => {
                match control {
                    PlayerControl::Pause => {
                        sink.lock().await.pause();
                        state_tx.send_modify(|state| { state.playing = Playing::Paused; });
                    },
                    PlayerControl::Resume => {
                        sink.lock().await.play();
                        state_tx.send_modify(|state| { state.playing = Playing::Playing; });
                    },
                    PlayerControl::SkipAll => reset_sink().await,
                    PlayerControl::SkipOne => {
                        if let Some(queued_song) = queue.pop_front() {
                            // Resend the updated queue
                            queued_song_tx.send_modify(|queue| {queue.pop_front();});

                            // Ask for the playlist AOT
                            let playlist = downloader.media_playlist(queued_song).await.unwrap();

                            // Calculate the total length of the track
                            let total = playlist.segments.iter().map(|x| x.duration).sum::<f32>();

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
                                    let state_tx = state_tx.clone();
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
                    PlayerControl::Queue(id) => {
                        info!("Queuing track");
                        queue.push_back(id);
                        queued_song_tx
                            .send(queue.clone())
                            .unwrap();
                        if queue.len() == 1 && sink.lock().await.empty() {
                            let loop_control_tx = loop_control_tx.clone();
                            tokio::spawn(async move {loop_control_tx.send(PlayerControl::SkipOne).await.unwrap()});
                        }
                    }
                    PlayerControl::QueueMany(ids) => {
                        info!("Queuing many");
                        queue.extend(ids.iter());
                        queued_song_tx.send_modify(|queue| queue.extend(ids));
                        if sink.lock().await.empty() {
                            let loop_control_tx = loop_control_tx.clone();
                            tokio::spawn(async move {loop_control_tx.send(PlayerControl::SkipOne).await.unwrap()});
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
