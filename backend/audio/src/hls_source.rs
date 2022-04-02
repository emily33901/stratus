use std::{
    io::{self, Read},
    sync::Arc,
    task::Poll,
};

use derive_more::{Deref, DerefMut};
use log::info;
use tokio::io::AsyncRead;
use tokio::sync::mpsc::Receiver;
// use tokio::sync::RwLock;
use parking_lot::RwLock;

#[derive(Debug)]
pub(crate) struct _HlsReader {
    rx: Receiver<Vec<u8>>,
    store: Vec<u8>,
}

#[repr(transparent)]
#[derive(Clone, Debug, Deref)]
pub(crate) struct HlsReader(Arc<RwLock<_HlsReader>>);

impl HlsReader {
    pub(crate) fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self(Arc::new(RwLock::new(_HlsReader { rx, store: vec![] })))
    }
}

impl AsyncRead for HlsReader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        {
            // See if we have anything in the store to hand over
            let mut zelf = self.write();

            if zelf.store.len() > 0 {
                // We have some existing data that we need to flush out
                let len = zelf.store.len().min(buf.remaining());
                buf.put_slice(&zelf.store[..len]);
                zelf.store.drain(..len);
            }
        }

        if buf.remaining() == 0 {
            // Early out as the buffer is full
            return Poll::Ready(Ok(()));
        }

        let pr = self.write().rx.poll_recv(cx);
        match pr {
            Poll::Ready(Some(chunk)) => {
                // Figure out what len we can put into the buffer
                let len = buf.remaining().min(chunk.len());
                buf.put_slice(&chunk[..len]);
                // Put the rest into the store for next time
                self.write().store.extend(&chunk[len..]);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}
