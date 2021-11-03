use std::{
    collections::VecDeque,
    io::{self, Cursor},
    sync::{
        atomic::{self, AtomicUsize},
        Arc, Mutex, RwLock, Weak,
    },
};

#[derive(Default, Clone)]
pub struct HlsReader {
    buffer: Arc<Mutex<Vec<u8>>>,
    pos: usize,
}

impl HlsReader {
    pub fn add(&self, data: &[u8]) {
        self.buffer.lock().unwrap().extend(data);
    }

    pub fn len(&self) -> usize {
        self.buffer.lock().unwrap().len()
    }
}

impl io::Read for HlsReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len();
        let buffer = self.buffer.lock().unwrap();
        if len > buffer.len() - self.pos {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "HlsReader needs more data",
            ));
        }
        buf.copy_from_slice(&buffer[self.pos..self.pos + len]);
        self.pos += len;
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
            io::SeekFrom::Current(pos) => {
                self.pos = (self.pos as i64 + pos) as usize;
                Ok(self.pos as u64)
            }
        }
    }
}
