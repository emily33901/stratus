use std::sync::Arc;

use iced::pure::{button, row, text, widget::Column, Element};
use iced::Length;

use crate::sc::{self};

use super::{app::Message, cache::ImageCache};

pub struct Song {
    song: sc::Song,
    pub user: Option<sc::User>,
    image_cache: Arc<ImageCache>,
}

impl Song {
    pub fn view(&self) -> Element<Message> {
        {
            if let Some(image) = self.image_cache.image_for_song(&self.song) {
                row().push(image.width(Length::Units(100)))
            } else {
                row()
            }
            .push(
                row()
                    .push(
                        Column::new()
                            .push(text(&self.song.title))
                            .push(
                                button(text(
                                    self.user
                                        .as_ref()
                                        .map(|user| user.username.clone())
                                        .unwrap_or_default(),
                                ))
                                .on_press(
                                    self.user
                                        .as_ref()
                                        .map(|user| Message::UserClicked(user.clone()))
                                        .or(Some(Message::none()))
                                        .unwrap(),
                                ),
                            )
                            .spacing(20)
                            .width(Length::Shrink),
                    )
                    .width(Length::Fill)
                    .spacing(20)
                    .push(
                        button(text("Add to queue"))
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

    pub fn username(&self) -> Option<&str> {
        self.user.as_ref().map(|user| user.username.as_ref())
    }
}

impl Song {
    pub fn new(song: sc::Song, image_cache: Arc<ImageCache>) -> Self {
        Self {
            song,
            user: None,
            image_cache,
        }
    }
}
