use std::{
    io::{self, Read, Seek},
    sync::Arc,
};

use derive_more::{Deref, DerefMut};
use log::info;
use parking_lot::{Mutex, RwLock};
use tokio::sync::mpsc;

#[derive(Debug, Deref, DerefMut)]
pub struct _HlsReader {
    #[deref]
    #[deref_mut]
    buffer: Vec<u8>,
    chunk_rx: Arc<mpsc::Receiver<Vec<u8>>>,
}

#[repr(transparent)]
#[derive(Clone, Debug, Deref, DerefMut)]
pub struct HlsReader(Arc<RwLock<_HlsReader>>);

impl HlsReader {
    pub(crate) fn new(chunk_rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self(Arc::new(RwLock::new(_HlsReader {
            buffer: vec![],
            chunk_rx: Arc::new(chunk_rx),
        })))
    }

    fn fill(&self, wanted: usize) {
        if self.0.read().buffer.len() >= wanted {
            return;
        }

        let chunk_rx = self.0.read().chunk_rx.clone();

        let zelf = self.clone();
        tokio::runtime::Handle::current().spawn(async move {
            if let Some(chunk) = chunk_rx.recv().await {
                zelf.write().buffer.extend(chunk);
            }
            // zelf.write()
            //     .chunk_rx
            //     .recv()
            //     .await
            //     .map(|chunk| self.write().buffer.extend(chunk))
        });

        // if let Some(next_buffer) =
        //     tokio::runtime::Handle::current().block_on(self.write().chunk_rx.recv())
        // {
        //     self.write().extend(&next_buffer)
        // } else {
        //     info!("Attempted to fill() but recvd error from chunk_rx");
        // }
    }
}

impl Read for HlsReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Try and fill
        self.fill(buf.len());

        let mut vec = self.write();
        let res = vec.as_slice().read(buf);
        if let Ok(read) = &res {
            vec.drain(..*read);
        }
        res
    }
}

// #[derive(Default, Clone, Debug)]
// pub struct HlsReader {
//     buffer: Arc<RwLock<Vec<u8>>>,
//     pos: usize,
// }

// impl HlsReader {
//     pub fn add(&self, data: &[u8]) {
//         self.buffer.write().extend(data);
//     }

//     pub fn len(&self) -> usize {
//         self.buffer.read().len()
//     }
// }

// impl io::Read for HlsReader {
//     fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
//         let len = buf.len();
//         let buffer = self.buffer.read();
//         if len > buffer.len() - self.pos {
//             warn!("Exhasusted hls buffer");
//             return Err(io::Error::new(
//                 io::ErrorKind::WouldBlock,
//                 "HlsReader needs more data",
//             ));
//         }
//         let pos = self.pos;
//         self.pos += len;
//         buf.copy_from_slice(&buffer[pos..pos + len]);
//         Ok(len)
//     }
// }

// impl io::Seek for HlsReader {
//     fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
//         match pos {
//             io::SeekFrom::Start(pos) => {
//                 self.pos = pos as usize;
//                 Ok(pos)
//             }
//             io::SeekFrom::End(pos) => todo!(),
//             io::SeekFrom::Current(delta) => {
//                 let pos = self.pos;
//                 self.pos = (self.pos as i64 + delta) as usize;
//                 Ok(self.pos as u64)
//             }
//         }
//     }
// }
