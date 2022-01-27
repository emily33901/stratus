mod hls_source;
mod mp3;

use std::{
    collections::{HashMap, VecDeque},
    sync::{atomic::AtomicUsize, Arc},
    thread, time,
};

use async_trait::async_trait;
use eyre::Result;
use hls_source::{HlsReader, LazyReader};
use log::{info, warn};
use m3u8_rs::playlist::MediaPlaylist;
use rodio::{Decoder, Source};
use tokio::{sync::mpsc, sync::oneshot, sync::watch, sync::Mutex};

use crate::mp3::HlsDecoder;

#[async_trait]
pub trait Downloader: Send + Sync {
    async fn download(&self, url: &str) -> Result<Vec<u8>>;
}

#[derive(Debug)]
enum PlayerControl {
    Pause,
    Resume,
    SkipAll,
    SkipOne,
    Volume(f32),
    Seek(usize),
    Queue(LazyReader),
}

pub struct HlsPlayer {
    control: mpsc::Sender<PlayerControl>,
    pos: Arc<AtomicUsize>,
    total: parking_lot::Mutex<f32>,
}

impl HlsPlayer {
    pub fn new(downloader: Arc<dyn Downloader>) -> Self {
        let (control_tx, mut control_rx) = mpsc::channel(10);
        let loop_control_tx = control_tx.clone();

        let pos = Arc::new(AtomicUsize::new(0));
        let loop_pos = pos.clone();

        thread::spawn(move || -> Result<_> {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
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

                    let mut queue = VecDeque::<LazyReader>::new();

                    // let pos = AtomicUsize::new(0);
                    // let total = Arc::new(0_usize);

                    // let build_decoder = |reader| -> _ {
                    //     let decoder = HlsDecoder::new(reader).unwrap();
                    //     decoder.periodic_access(time::Duration::from_millis(100), move |decoder| {
                    //         pos.store(decoder.samples(), std::sync::atomic::Ordering::Release);
                    //     })
                    // };

                    while let Some(control) = control_rx.recv().await {
                        let downloader = downloader.clone();
                        match control {
                            PlayerControl::Pause => sink.lock().await.pause(),
                            PlayerControl::Resume => sink.lock().await.play(),
                            PlayerControl::SkipAll => sink.lock().await.stop(),
                            PlayerControl::SkipOne => {
                                if let Some(playlist) = queue.pop_front() {
                                    let reader = HlsReader::default();
                                    let r2 = reader.clone();

                                    let (ready_tx, mut ready_rx) = mpsc::channel::<()>(1);

                                    tokio::spawn(async move {
                                        for (i, s) in playlist.playlist.segments.iter().enumerate()
                                        {
                                            let downloaded =
                                                downloader.download(&s.uri).await.unwrap();
                                            r2.add(&downloaded);

                                            info!("downloaded {i}");

                                            if i == 1 {
                                                ready_tx.send(()).await.unwrap();
                                            }
                                        }
                                    });
                                    ready_rx.recv().await;

                                    reset_sink().await;
                                    let decoder = HlsDecoder::new(reader).unwrap();
                                    let loop_pos = loop_pos.clone();
                                    let periodic = decoder.periodic_access(
                                        time::Duration::from_millis(100),
                                        move |decoder| {
                                            loop_pos.store(
                                                decoder.samples(),
                                                std::sync::atomic::Ordering::Relaxed,
                                            );
                                        },
                                    );
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
                                if queue.len() == 1 && sink.lock().await.empty() {
                                    loop_control_tx.send(PlayerControl::SkipOne).await.unwrap();
                                }
                            }
                            PlayerControl::Volume(_) => todo!(),
                            PlayerControl::Seek(_) => todo!(),
                        }
                    }
                });

            Ok(())
        });

        Self {
            pos,
            total: Default::default(),
            control: control_tx,
        }
    }

    #[cfg(target = "never")]
    pub async fn download(&self) -> Result<()> {
        // Try and download all segments of the playlist and append them to decoder.
        let reader = HlsReader::default();

        for (i, s) in self.playlist.segments.iter().enumerate() {
            let downloaded = self.downloader.download(&s.uri).await?;
            reader.add(&downloaded);

            if i == 0 {
                let decoder = HlsDecoder::new(reader.clone())?;
                let pos = self.pos.clone();
                let periodic =
                    decoder.periodic_access(time::Duration::from_millis(100), move |decoder| {
                        pos.store(decoder.samples(), std::sync::atomic::Ordering::Release);
                    });
                self.sink.lock().await.append(periodic);
            }

            info!("Appended {} successfully ({})", i, reader.len());
            *self.total.lock() += s.duration;
            tokio::task::yield_now().await;
        }
        info!("Done downloading");
        Ok(())
    }

    pub async fn queue(&self, playlist: &str) -> Result<()> {
        let bytes = playlist.as_bytes().to_vec();
        let playlist = m3u8_rs::parse_media_playlist_res(&bytes).unwrap();

        Ok(self
            .control
            .send(PlayerControl::Queue(LazyReader::new(playlist)))
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
}
