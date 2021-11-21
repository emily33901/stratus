use std::{
    io::{Read, Seek},
    time,
};

use eyre::Result;
use minimp3::{Decoder, Frame};
use rodio::Source;

use crate::hls_source::HlsReader;

pub struct HlsDecoder<R>
where
    R: Read + Seek,
{
    decoder: Decoder<R>,
    current_frame: Frame,
    current_frame_offset: usize,
    elapsed: usize,
}

impl<R: Read + Seek> HlsDecoder<R> {
    pub fn new(data: R) -> Result<Self> {
        let mut decoder = Decoder::new(data);
        let current_frame = decoder.next_frame()?;

        Ok(HlsDecoder {
            decoder,
            current_frame,
            current_frame_offset: 0,
            elapsed: 0,
        })
    }

    pub fn samples(&self) -> usize {
        self.current_frame_offset + self.elapsed
    }
}

impl<R> Source for HlsDecoder<R>
where
    R: Read + Seek,
{
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

impl<R> Iterator for HlsDecoder<R>
where
    R: Read + Seek,
{
    type Item = i16;

    #[inline]
    fn next(&mut self) -> Option<i16> {
        if self.current_frame_offset == self.current_frame.data.len() {
            match self.decoder.next_frame() {
                Ok(frame) => self.current_frame = frame,
                _ => return None,
            }
            self.elapsed += self.current_frame_offset;
            self.current_frame_offset = 0;
        }

        let v = self.current_frame.data[self.current_frame_offset];
        self.current_frame_offset += 1;

        Some(v)
    }
}
