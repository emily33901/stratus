mod hls_source;

use std::{
    sync::{mpsc, Arc},
    thread, time,
};

use async_trait::async_trait;
use eyre::Result;
use hls_source::HlsReader;
use log::{info, warn};
use m3u8_rs::playlist::MediaPlaylist;
use rodio::{Decoder, Source};
use tokio::{sync::oneshot, sync::watch, sync::Mutex};

#[async_trait]
pub trait Downloader: Send + Sync {
    async fn download(&self, url: &str) -> Result<Vec<u8>>;
}

pub struct HlsPlayer {
    playlist: MediaPlaylist,
    // TODO(emily): temp pub
    pub sink: Arc<Mutex<rodio::Sink>>,
    downloader: Box<dyn Downloader>,
    die_tx: Option<oneshot::Sender<()>>,
    pub position_tx: Mutex<Option<watch::Sender<usize>>>,
    pub position_rx: watch::Receiver<usize>,
}

impl HlsPlayer {
    pub fn new(playlist: &str, downloader: Box<dyn Downloader>) -> Result<Self> {
        let bytes = playlist.as_bytes().to_vec();
        let playlist = m3u8_rs::parse_media_playlist_res(&bytes).unwrap();
        // get the default output device

        // NOTICE(emily): this is absolutely fucked I should not be forced
        // to do such fuckery for a fucking audio output
        let (tx, rx) = mpsc::channel();
        let (die_tx, die_rx) = oneshot::channel();
        thread::spawn(move || -> Result<_> {
            let (stream, handle) = rodio::OutputStream::try_default()?;
            let sink = rodio::Sink::try_new(&handle)?;
            tx.send(sink)?;

            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
                .block_on(async {
                    die_rx.await.unwrap_or(());
                });

            Ok(())
        });

        let sink = rx.recv()?;

        let (position_tx, position_rx) = watch::channel(10);

        Ok(Self {
            playlist,
            sink: Arc::new(Mutex::new(sink)),
            die_tx: Some(die_tx),
            downloader,
            position_tx: Mutex::new(Some(position_tx)),
            position_rx,
        })
    }

    pub async fn download(&self) -> Result<()> {
        // Try and download all segments of the playlist and append them to the sink
        let reader = HlsReader::default();

        for (i, s) in self.playlist.segments.iter().enumerate() {
            let downloaded = self.downloader.download(&s.uri).await?;
            reader.add(&downloaded);

            if i == 0 {
                let decoder = Decoder::new_mp3(reader.clone())?;
                let reader = reader.clone();
                let sender = self.position_tx.lock().await.take().unwrap();
                let periodic =
                    decoder.periodic_access(time::Duration::from_millis(100), move |_| {
                        let reader_pos = reader.pos();
                        sender.send(reader_pos).unwrap();
                    });
                self.sink.lock().await.append(periodic);
            }

            info!("Appended {} successfully ({})", i, reader.len());
        }
        info!("Done downloading");
        Ok(())
    }

    pub async fn resume(&self) {
        info!("Resuming playback");
        let sink = self.sink.lock().await;
        sink.set_volume(1.0);
        sink.play();
    }

    pub async fn pause(&self) {
        info!("Pausing playback");
        let sink = self.sink.lock().await;
        sink.pause();
    }

    pub async fn stop(&self) -> Result<()> {
        info!("Stopping playback");
        let sink = self.sink.lock().await;
        sink.stop();
        Ok(())
    }

    pub async fn position(&self) -> usize {
        *self.position_rx.borrow()
    }
}

impl Drop for HlsPlayer {
    fn drop(&mut self) {
        if let Some(die) = self.die_tx.take() {
            die.send(()).unwrap();
        }
    }
}
