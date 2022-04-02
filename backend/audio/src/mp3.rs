use std::{
    io::{Read, Seek},
    time,
};

use eyre::Result;
use minimp3::{Decoder, Frame};
use rodio::Source;
use tokio::sync::mpsc;

use crate::hls_source::HlsReader;

pub struct HlsDecoder {
    reader: HlsReader,
    decoder: Decoder<HlsReader>,
    current_frame: Frame,
    current_frame_offset: usize,
    elapsed: usize,
    finished_signal: tokio::sync::mpsc::Sender<()>,
}

impl HlsDecoder {
    pub fn new(
        chunk_rx: mpsc::Receiver<Vec<u8>>,
        finished_signal: &tokio::sync::mpsc::Sender<()>,
    ) -> Result<Self> {
        let reader = HlsReader::new(chunk_rx);
        let mut decoder = Decoder::new(reader.clone());

        // Make sure that we have a frame ready to go
        let current_frame = decoder.next_frame()?;

        Ok(HlsDecoder {
            reader,
            decoder,
            current_frame,
            current_frame_offset: 0,
            elapsed: 0,
            finished_signal: finished_signal.clone(),
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
            match self.decoder.next_frame() {
                Ok(frame) => self.current_frame = frame,
                _ => {
                    self.finished_signal.blocking_send(()).unwrap();
                    return None;
                }
            }
            self.elapsed += self.current_frame_offset;
            self.current_frame_offset = 0;
        }

        let v = self.current_frame.data[self.current_frame_offset];
        self.current_frame_offset += 1;

        Some(v)
    }
}
