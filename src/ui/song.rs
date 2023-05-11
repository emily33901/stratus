use std::sync::Arc;

use iced::widget;
use iced::Element;
use iced::Length;

use crate::model;

use super::app::Message;

pub struct Song {
    song: Arc<model::Song>,
}

impl Song {
    pub fn view(&self) -> Element<Message> {
        {
            if let Some(image) = &self.song.artwork {
                widget::row!().push(
                    widget::image::Image::new(image.as_ref().clone()).width(Length::Fixed(100.0)),
                )
            } else {
                widget::row!()
            }
            .push(
                widget::row!()
                    .push(
                        widget::column!()
                            .push(widget::text(&self.song.title))
                            .push(
                                widget::button(widget::text(self.song.user.username.clone()))
                                    .on_press(Message::UserClicked(self.song.user.clone())),
                            )
                            .spacing(20)
                            .width(Length::Shrink),
                    )
                    .width(Length::Fill)
                    .spacing(20)
                    .push(
                        widget::button(widget::text("Add to queue"))
                            .on_press(Message::SongQueue(self.song.clone())),
                    ),
            )
        }
        .spacing(20)
        .into()
    }

    pub fn title(&self) -> &str {
        &self.song.title
    }

    pub fn user_id(&self) -> model::Id {
        self.song.user.id
    }

    pub fn song(&self) -> &Arc<model::Song> {
        &self.song
    }

    pub fn username(&self) -> &str {
        self.song.user.username.as_ref()
    }
}

impl Song {
    pub fn new(song: Arc<model::Song>) -> Self {
        Self { song }
    }
}
