use std::{error::Error, time};

use eyre::Result;
use log::info;
use minimp3::{Decoder, Frame};
use rodio::Source;
use tokio::sync::mpsc;

use crate::hls_source::HlsReader;

pub struct HlsDecoder {
    current_frame: Frame,
    next_frame_rx: mpsc::Receiver<Frame>,
    current_frame_offset: usize,
    elapsed: usize,
    finished_signal: tokio::sync::mpsc::Sender<()>,
}

impl HlsDecoder {
    pub async fn new(
        chunk_rx: mpsc::Receiver<Vec<u8>>,
        finished_signal: &tokio::sync::mpsc::Sender<()>,
    ) -> Result<Self> {
        let (next_frame_tx, mut next_frame_rx) = mpsc::channel(30);

        tokio::spawn(async move {
            let mut decoder = Decoder::new(HlsReader::new(chunk_rx));
            loop {
                match { decoder.next_frame_future().await } {
                    Ok(frame) => {
                        if let Err(err) = next_frame_tx.send(frame).await {
                            info!("next_frame_rx gone. {:?}", err.source());
                            break;
                        }
                    }
                    Err(err) => {
                        info!("Error getting next frame: {:?}", err);
                        // TODO(emily): Probably want to be doing something better here
                        break;
                    }
                }
            }
        });

        // Make sure that we have a frame ready to go
        let current_frame = next_frame_rx.recv().await.unwrap();

        Ok(HlsDecoder {
            current_frame,
            current_frame_offset: 0,
            elapsed: 0,
            finished_signal: finished_signal.clone(),
            next_frame_rx,
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
                if let Some(frame) = self.next_frame_rx.try_recv().ok() {
                    frame
                } else {
                    // TODO(emily): We need a different signal for this.
                    // This could either be the end or it could just be that we failed to read the next chunk
                    info!("No next frame. Sending finished signal");
                    self.finished_signal.blocking_send(()).unwrap();
                    return None;
                }
            };
        }

        let v = self.current_frame.data[self.current_frame_offset];
        self.current_frame_offset += 1;

        Some(v)
    }
}
