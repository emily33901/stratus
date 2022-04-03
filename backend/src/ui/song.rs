use std::sync::Arc;

use iced::{Button, Column, Element, Length, Row, Text};

use crate::sc::{self, Id};

use super::{app::Message, cache::ImageCache};

pub struct Song {
    song: sc::Song,
    pub user: Option<sc::User>,
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
            .push(
                Row::new()
                    .push(
                        Column::new()
                            .push(Text::new(&self.song.title))
                            .push(Text::new(
                                self.user
                                    .as_ref()
                                    .map(|user| user.username.clone())
                                    .unwrap_or_default(),
                            ))
                            .spacing(20)
                            .width(Length::Shrink),
                    )
                    .width(Length::Fill)
                    .spacing(20)
                    .push(
                        Button::new(&mut self.play_button_state, Text::new("Add to queue"))
                            .on_press(Message::SongQueue(self.song.clone()))
                            .style(crate::ui::style::Theme::Dark),
                    ),
            )
        }
        .spacing(20)
        .into()
    }

    pub fn title(&self) -> &str {
        &self.song.title
    }

    pub fn user_id(&self) -> sc::api::model::Id {
        self.song.user.id
    }

    pub fn song(&self) -> &sc::api::model::Song {
        &self.song
    }

    pub fn username(&self) -> Option<&String> {
        self.user.as_ref().map(|user| &user.username)
    }
}

impl Song {
    pub fn new(song: sc::Song, image_cache: Arc<ImageCache>) -> Self {
        Self {
            song,
            user: None,
            image_cache,
            play_button_state: iced::button::State::new(),
        }
    }
}
