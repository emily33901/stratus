use std::{io, sync::Arc};

use log::warn;
use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc;

#[derive(Default, Clone)]
pub struct HlsReader {
    buffer: Arc<RwLock<Vec<u8>>>,
    pos: Arc<RwLock<usize>>,
}

impl HlsReader {
    pub fn add(&self, data: &[u8]) {
        self.buffer.write().extend(data);
    }

    pub fn len(&self) -> usize {
        self.buffer.read().len()
    }

    pub fn pos(&self) -> usize {
        *self.pos.read()
    }
}

impl io::Read for HlsReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len();
        let buffer = self.buffer.read();
        let mut pos = self.pos.write();
        if len > buffer.len() - *pos {
            warn!("Exhasusted hls buffer");
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "HlsReader needs more data",
            ));
        }
        buf.copy_from_slice(&buffer[*pos..*pos + len]);
        *pos += len;
        Ok(len)
    }
}

impl io::Seek for HlsReader {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let mut spos = self.pos.write();
        match pos {
            io::SeekFrom::Start(pos) => {
                *spos = pos as usize;
                Ok(pos)
            }
            io::SeekFrom::End(pos) => todo!(),
            io::SeekFrom::Current(pos) => {
                *spos = (*spos as i64 + pos) as usize;
                Ok(*spos as u64)
            }
        }
    }
}
