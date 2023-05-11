use std::{
    io::{self},
    task::Poll,
};

use tokio::io::AsyncRead;
use tokio::sync::mpsc::Receiver;

#[derive(Debug)]
pub(crate) struct HlsReader {
    rx: Receiver<Vec<u8>>,
    store: Vec<u8>,
}

impl HlsReader {
    pub(crate) fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self { rx, store: vec![] }
    }
}

const STORE_LOW_MARK: usize = 10000;

impl AsyncRead for HlsReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        if self.store.len() < STORE_LOW_MARK {
            // Try and fill up the store
            let pr = self.rx.poll_recv(cx);
            match pr {
                Poll::Ready(Some(chunk)) => self.store.extend(&chunk),
                Poll::Ready(None) => {
                    // No more data for the store...
                    // Hand over as much as we can
                    let len = self.store.len().min(buf.remaining());
                    buf.put_slice(&self.store[..len]);
                    self.store.drain(..len);
                    return Poll::Ready(Ok(()));
                }
                // Return pending if we are pending...
                Poll::Pending => return Poll::Pending,
            }
        }

        // See if we have anything in the store to hand over
        if self.store.len() > 0 {
            // We have some existing data that we need to flush out
            let len = self.store.len().min(buf.remaining());
            buf.put_slice(&self.store[..len]);
            self.store.drain(..len);
        }
        Poll::Ready(Ok(()))
    }
}
