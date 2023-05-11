use std::{collections::VecDeque, ops::RangeInclusive, sync::Arc, time};

use iced::{Command, Element};

use crate::model::{self, Store};

use super::app::Message;

use ellipse::Ellipse;

pub struct ControlsElement {
    options: Vec<String>,
    cur_song_title: Option<String>,
}

impl ControlsElement {
    pub fn new() -> Self {
        Self {
            cur_song_title: None,
            options: Default::default(),
        }
    }

    pub fn queue_changed(&mut self, queue: &[Arc<model::Song>]) -> iced::Command<Message> {
        self.options = queue
            .iter()
            .map(|s| s.title.as_str().truncate_ellipse(30).into())
            .collect();

        Command::none()
    }

    pub fn set_cur_song_title(&mut self, title: Option<String>) {
        self.cur_song_title = title;
    }

    pub fn view(&self, location: time::Duration, total: time::Duration) -> Element<Message> {
        let queue = iced::widget::container(iced::widget::pick_list(
            &self.options,
            self.cur_song_title.clone(),
            |_x| Message::none(),
        ));

        iced::widget::row!()
            .push(iced::widget::button(iced::widget::text("play")).on_press(Message::Resume))
            .push(iced::widget::button(iced::widget::text("pause")).on_press(Message::Pause))
            .push(iced::widget::button(iced::widget::text("skip")).on_press(Message::Skip))
            .push(iced::widget::text(format!("{:.1}", location.as_secs_f32())))
            .push(iced::widget::slider(
                RangeInclusive::new(0.0, total.as_secs_f64()),
                location.as_secs_f64(),
                |_| Message::None(()),
            ))
            .push(iced::widget::text(format!("{:.1}", total.as_secs_f32())))
            .push(queue)
            .align_items(iced::Alignment::Center)
            .spacing(20)
            .padding(iced::Padding::new(20.0))
            .into()
    }
}
