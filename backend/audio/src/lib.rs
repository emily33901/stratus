mod hls_source;

use core::time;
use std::{
    io::{BufReader, Cursor},
    sync::{mpsc, Arc, Mutex},
    thread::{self},
};

use async_trait::async_trait;
use eyre::Result;
use log::{debug, info, warn};
use m3u8_rs::playlist::{MediaPlaylist, Playlist};
use rodio::{buffer::SamplesBuffer, queue::SourcesQueueOutput, Decoder, Source};

#[async_trait]
pub trait Downloader: Send + Sync {
    async fn download(&self, url: &str) -> Result<Vec<u8>>;
}

pub struct HlsPlayer {
    playlist: MediaPlaylist,
    // TODO(emily): temp pub
    pub sink: rodio::Sink,
    downloader: Box<dyn Downloader>,
    done: Arc<Mutex<bool>>,
}

impl HlsPlayer {
    pub fn new(playlist: &str, downloader: Box<dyn Downloader>) -> Result<Self> {
        let bytes = playlist.as_bytes().to_vec();
        let playlist = m3u8_rs::parse_media_playlist_res(&bytes).unwrap();
        // get the default output device

        // NOTICE(emily): this is absolutely fucked I should not be forced
        // to do such fuckery for a fucking audio output
        let (tx, rx) = mpsc::channel();
        let done = Arc::new(Mutex::new(false));
        let tdone = done.clone();
        thread::spawn(move || -> Result<_> {
            let (stream, handle) = rodio::OutputStream::try_default()?;
            let sink = rodio::Sink::try_new(&handle)?;
            tx.send(sink)?;

            loop {
                {
                    let done = tdone.lock().unwrap();
                    if *done {
                        break;
                    }
                }

                thread::sleep(time::Duration::from_millis(100));
            }

            Ok(())
        });

        let sink = rx.recv()?;

        Ok(Self {
            playlist,
            sink,
            done,
            downloader,
        })
    }

    pub async fn download(&self) -> Result<()> {
        // Try and download all segments of the playlist and append them to the sink
        for (i, s) in self.playlist.segments[..self.playlist.segments.len() - 1]
            .iter()
            .enumerate()
        {
            let downloaded = self.downloader.download(&s.uri).await?;
            // let downloaded = { self.downloader.download(&s.uri).await? };
            std::fs::write(&format!("test/test_{}.mp3", i), &downloaded)?;
            let cursor = Cursor::new(downloaded);
            let decoder = Decoder::new_mp3(cursor)
                .map_err(|err| {
                    warn!("Decoder for {} failed: {}", i, err);
                    err
                })?
                .periodic_access(time::Duration::from_millis(5), move |src| {});
            self.sink.append(decoder);
            warn!("Appended {} successfully", i);
        }
        info!("Done downloading");
        Ok(())
    }

    pub fn play(&self) {
        info!("Beginning playback");
        self.sink.set_volume(1.0);
        self.sink.play();
    }

    pub fn pause(&self) {
        info!("Pausing playback");
        self.sink.pause();
    }

    pub fn stop(&self) -> Result<()> {
        info!("Stopping playback");
        self.sink.stop();
        let mut done = self.done.lock().unwrap();
        *done = true;
        Ok(())
    }
}
