use std::{io, sync::Arc};

use log::warn;
use m3u8_rs::playlist::{MediaPlaylist, Playlist};
use parking_lot::{Mutex, RwLock};

#[derive(Default, Clone)]
pub struct HlsReader {
    buffer: Arc<RwLock<Vec<u8>>>,
    pos: usize,
}

impl HlsReader {
    pub fn add(&self, data: &[u8]) {
        self.buffer.write().extend(data);
    }

    pub fn len(&self) -> usize {
        self.buffer.read().len()
    }
}

impl io::Read for HlsReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len();
        let buffer = self.buffer.read();
        if len > buffer.len() - self.pos {
            warn!("Exhasusted hls buffer");
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "HlsReader needs more data",
            ));
        }
        let pos = self.pos;
        self.pos += len;
        buf.copy_from_slice(&buffer[pos..pos + len]);
        Ok(len)
    }
}

impl io::Seek for HlsReader {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match pos {
            io::SeekFrom::Start(pos) => {
                self.pos = pos as usize;
                Ok(pos)
            }
            io::SeekFrom::End(pos) => todo!(),
            io::SeekFrom::Current(delta) => {
                let pos = self.pos;
                self.pos = (self.pos as i64 + delta) as usize;
                Ok(self.pos as u64)
            }
        }
    }
}

pub(crate) struct LazyReader {
    downloader: Box<dyn super::Downloader>,
    playlist: MediaPlaylist,
    runtime: tokio::runtime::Handle,
    reader: Option<HlsReader>,
}

impl LazyReader {
    pub(crate) fn new(
        playlist: MediaPlaylist,
        downloader: Box<dyn super::Downloader>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        LazyReader {
            playlist,
            downloader,
            runtime,
            reader: None,
        }
    }

    fn populate_reader(&mut self) {
        self.reader = Some(HlsReader::default())
    }

    #[cfg(target = "never")]
    async fn download(&self) -> Result<()> {
        todo!();

        // Try and download all segments of the playlist and append them to decoder.
        let reader = HlsReader::default();

        for (i, s) in self.playlist.segments.iter().enumerate() {
            let downloaded = self.downloader.download(&s.uri).await?;
            reader.add(&downloaded);

            if i == 0 {
                let decoder = HlsDecoder::new(reader.clone())?;
                // let decoder = Decoder::new_mp3(reader.clone())?;
                let sender = self.position_tx.lock().await.take().unwrap();
                let periodic =
                    decoder.periodic_access(time::Duration::from_millis(100), move |decoder| {
                        let pos = decoder.samples();
                        sender.send(pos).unwrap();
                    });
                self.sink.lock().await.append(periodic);
            }

            info!("Appended {} successfully ({})", i, reader.len());
        }
        info!("Done downloading");
        Ok(())
    }
}

impl io::Read for LazyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(reader) = self.reader.as_mut() {
            reader.read(buf)
        } else {
            self.populate_reader();
            self.reader.as_mut().unwrap().read(buf)
        }
    }
}

impl io::Seek for LazyReader {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        if let Some(reader) = self.reader.as_mut() {
            reader.seek(pos)
        } else {
            self.populate_reader();
            self.reader.as_mut().unwrap().seek(pos)
        }
    }
}
