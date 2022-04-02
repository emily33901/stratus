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

impl AsyncRead for HlsReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        {
            // See if we have anything in the store to hand over
            if self.store.len() > 0 {
                // We have some existing data that we need to flush out
                let len = self.store.len().min(buf.remaining());
                buf.put_slice(&self.store[..len]);
                self.store.drain(..len);
            }
        }

        if buf.remaining() == 0 {
            // Early out as the buffer is full
            return Poll::Ready(Ok(()));
        }

        let pr = self.rx.poll_recv(cx);
        match pr {
            Poll::Ready(Some(chunk)) => {
                // Figure out what len we can put into the buffer
                let len = buf.remaining().min(chunk.len());
                buf.put_slice(&chunk[..len]);
                // Put the rest into the store for next time
                self.store.extend(&chunk[len..]);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}
