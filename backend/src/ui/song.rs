use std::sync::Arc;

use iced::{Button, Element, Length, Row, Text};

use crate::sc;

use super::{app::Message, cache::ImageCache};

pub struct Song {
    song: sc::Song,
    image_cache: Arc<ImageCache>,
    play_button_state: iced::button::State,
}

impl Song {
    pub fn view(&mut self) -> Element<Message> {
        {
            if let Some(image) = self.image_cache.image_for_song(&self.song) {
                Row::new().push(image.width(Length::Units(100)))
            } else {
                Row::new()
            }
            .push(Text::new(&self.song.title))
            .push(
                Button::new(&mut self.play_button_state, Text::new("play"))
                    .on_press(Message::SongPlay(self.song.clone())),
            )
        }
        .spacing(20)
        .into()
    }
}

impl Song {
    pub fn new(song: sc::Song, image_cache: Arc<ImageCache>) -> Self {
        Self {
            song,
            image_cache,
            play_button_state: iced::button::State::new(),
        }
    }
}
