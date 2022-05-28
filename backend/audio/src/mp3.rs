use std::{sync::Arc, time};

use eyre::Result;
use log::info;
use minimp3::{Decoder, Frame};
use parking_lot::RwLock;
use rodio::Source;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::hls_source::HlsReader;

pub struct HlsDecoder {
    decoder: Arc<Mutex<Decoder<HlsReader>>>,
    current_frame: Frame,
    next_frame: Arc<RwLock<Option<Frame>>>,
    current_frame_offset: usize,
    elapsed: usize,
    finished_signal: tokio::sync::mpsc::Sender<()>,
    runtime: tokio::runtime::Handle,
}

impl HlsDecoder {
    pub async fn new(
        chunk_rx: mpsc::Receiver<Vec<u8>>,
        finished_signal: &tokio::sync::mpsc::Sender<()>,
    ) -> Result<Self> {
        let decoder = Arc::new(Mutex::new(Decoder::new(HlsReader::new(chunk_rx))));

        // Make sure that we have a frame ready to go
        let current_frame = decoder.lock().await.next_frame_future().await?;
        let next_frame = decoder.lock().await.next_frame_future().await.ok();
        assert!(next_frame.is_some());
        let next_frame = Arc::new(RwLock::new(next_frame));

        Ok(HlsDecoder {
            decoder,
            current_frame,
            next_frame,
            current_frame_offset: 0,
            elapsed: 0,
            finished_signal: finished_signal.clone(),
            runtime: tokio::runtime::Handle::current(),
        })
    }

    pub fn samples(&self) -> usize {
        self.current_frame_offset + self.elapsed
    }
}

impl Source for HlsDecoder {
    #[inline]
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.current_frame.data.len())
    }

    #[inline]
    fn channels(&self) -> u16 {
        self.current_frame.channels as _
    }

    #[inline]
    fn sample_rate(&self) -> u32 {
        self.current_frame.sample_rate as _
    }

    #[inline]
    fn total_duration(&self) -> Option<time::Duration> {
        None
    }
}

impl Iterator for HlsDecoder {
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if self.current_frame_offset == self.current_frame.data.len() {
            // We reached the end of a frame :(
            // Here we swap the current frames around and queue decoding another
            // frame.
            self.elapsed += self.current_frame_offset;
            self.current_frame_offset = 0;

            self.current_frame = {
                if let Some(frame) = self.next_frame.write().take() {
                    frame
                } else {
                    // TODO(emily): We need a different signal for this.
                    // This could either be the end or it could just be that we failed to read the next chunk
                    info!("No next frame. Sending finished signal");
                    self.finished_signal.blocking_send(()).unwrap();
                    return None;
                }
            };

            let next_frame = self.next_frame.clone();
            let decoder = self.decoder.clone();
            self.runtime.spawn(async move {
                let f = decoder.lock().await.next_frame_future().await.ok();
                *next_frame.write() = f;
            });
        }

        let v = self.current_frame.data[self.current_frame_offset];
        self.current_frame_offset += 1;

        Some(v)
    }
}
