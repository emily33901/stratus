struct HlsSource {}

impl Iterator for HlsSource {
    type Item;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

impl rodio::Source for HlsSource {
    fn current_frame_len(&self) -> Option<usize> {
        todo!()
    }

    fn channels(&self) -> u16 {
        todo!()
    }

    fn sample_rate(&self) -> u32 {
        todo!()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        todo!()
    }
}
