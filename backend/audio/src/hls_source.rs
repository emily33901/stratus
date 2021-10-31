use std::{
    collections::VecDeque,
    sync::{
        atomic::{self, AtomicUsize},
        Arc, Mutex, RwLock, Weak,
    },
};

use eyre::Result;

pub struct HlsSource {
    channels: u16,
    sample_rate: u32,
    buffer: Arc<Mutex<Vec<i16>>>,
    r#where: usize,
}

impl HlsSource {
    pub fn new(
        initial: &mut dyn rodio::Source<Item = i16>,
        hls_length: f32,
    ) -> Result<(Self, Weak<Mutex<Vec<i16>>>)> {
        let channels = initial.channels();
        let sample_rate = initial.sample_rate();

        let buf_cap = hls_length as usize * initial.sample_rate() as usize;
        let buffer = Vec::with_capacity(buf_cap * 4);
        let abuffer = Arc::new(Mutex::new(buffer));

        Ok((
            Self {
                channels,
                sample_rate,
                buffer: abuffer.clone(),
                r#where: 0,
            },
            Arc::downgrade(&abuffer),
        ))
    }
}

impl Iterator for HlsSource {
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        let buffer = self.buffer.lock().unwrap();
        self.r#where += 1;
        Some(buffer[self.r#where])
    }
}

impl rodio::Source for HlsSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
        // Some(self.buffer.try_lock().unwrap().len())
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}
