use super::app::Message;
use crate::model::{self};
use ellipse::Ellipse;
use iced::{Command, Element, Length};
use std::{ops::RangeInclusive, sync::Arc, time};

pub struct ControlsElement {
    options: Vec<String>,
    cur_song: Option<Arc<model::Song>>,
}

impl ControlsElement {
    pub fn new() -> Self {
        Self {
            cur_song: None,
            options: vec![],
        }
    }

    pub fn queue_changed(&mut self, queue: &[Arc<model::Song>]) -> iced::Command<Message> {
        self.options = queue
            .iter()
            .map(|s| s.title.as_str().truncate_ellipse(30).into())
            .collect();

        Command::none()
    }

    pub fn set_cur_song(&mut self, song: Option<Arc<model::Song>>) {
        self.cur_song = song;
    }

    pub fn view(&self, location: time::Duration, total: time::Duration) -> Element<Message> {
        let queue = iced::widget::container(iced::widget::pick_list(
            &self.options,
            self.cur_song.as_ref().map(|s| s.title.clone()),
            |_x| Message::none(),
        ));

        iced::widget::row!(
            iced::widget::button(iced::widget::text("play")).on_press(Message::Resume),
            iced::widget::button(iced::widget::text("pause")).on_press(Message::Pause),
            iced::widget::button(iced::widget::text("skip")).on_press(Message::Skip),
            iced::widget::text(format!("{:.1}", location.as_secs_f32())),
            iced::widget::slider(
                RangeInclusive::new(0.0, total.as_secs_f64()),
                location.as_secs_f64(),
                |_| Message::None(()),
            ),
            iced::widget::text(format!("{:.1}", total.as_secs_f32())),
            if let Some(artwork) = self
                .cur_song
                .as_ref()
                .and_then(|s| s.artwork.clone())
                .as_ref()
            {
                iced::widget::container(
                    iced::widget::Image::new(artwork.as_ref().clone()).height(Length::Fixed(120.0)),
                )
            } else {
                iced::widget::container(iced::widget::text(""))
            },
            queue
        )
        .align_items(iced::Alignment::Center)
        .spacing(20)
        .into()
    }
}
