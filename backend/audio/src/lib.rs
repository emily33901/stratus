mod hls_source;
mod mp3;

use std::{
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
    playlist: MediaPlaylist,
    // TODO(emily): temp pub
    downloader: Box<dyn Downloader>,
    control: mpsc::Sender<PlayerControl>,
    pos: Arc<AtomicUsize>,
    total: parking_lot::Mutex<f32>,
}

impl HlsPlayer {
    pub fn new(playlist: &str, downloader: Box<dyn Downloader>) -> Result<Self> {
        let bytes = playlist.as_bytes().to_vec();
        let playlist = m3u8_rs::parse_media_playlist_res(&bytes).unwrap();

        let (control_tx, control_rx) = mpsc::channel(10);
        thread::spawn(move || -> Result<_> {
            let (stream, handle) = rodio::OutputStream::try_default()?;
            let sink = rodio::Sink::try_new(&handle)?;

            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(async move {
                    while let Some(control) = control_rx.recv().await {
                        match control {
                            PlayerControl::Pause => sink.pause(),
                            PlayerControl::Resume => sink.play(),
                            PlayerControl::SkipAll => sink.stop(),
                            PlayerControl::SkipOne => todo!(),
                            PlayerControl::Queue(playlist) => todo!(),
                            PlayerControl::Volume(_) => todo!(),
                            PlayerControl::Seek(_) => todo!(),
                        }
                    }
                });

            Ok(())
        });

        Ok(Self {
            playlist,
            downloader,
            pos: Default::default(),
            total: Default::default(),
            control: control_tx,
        })
    }

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

    pub fn position(&self) -> usize {
        self.pos.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn total(&self) -> f32 {
        *self.total.lock()
    }
}

impl Drop for HlsPlayer {
    fn drop(&mut self) {
        if let Some(die) = self.die_tx.take() {
            die.send(()).unwrap();
        }
    }
}
