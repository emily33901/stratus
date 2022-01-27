use std::{io, sync::Arc};

use eyre::Result;
use log::warn;
use m3u8_rs::playlist::{MediaPlaylist, Playlist};
use parking_lot::{Mutex, RwLock};

#[derive(Default, Clone, Debug)]
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
