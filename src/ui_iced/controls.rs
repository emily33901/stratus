use super::app::Message;
use crate::model::{self};
use ellipse::Ellipse;
use iced::{Command, Element, Length};
use std::{ops::RangeInclusive, sync::Arc, time};

pub struct ControlsElement {
    options: Vec<String>,
    cur_song: Option<Arc<model::Song>>,
    player_state: audio::PlayerState,
}

impl ControlsElement {
    pub fn new() -> Self {
        Self {
            cur_song: None,
            options: vec![],
            player_state: Default::default(),
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

    pub fn view(&self) -> Element<Message> {
        let queue = iced::widget::container(iced::widget::pick_list(
            &self.options,
            self.cur_song.as_ref().map(|s| s.title.clone()),
            |_x| Message::none(),
        ));

        use std::time::Duration;

        // TODO(emily): Conversion to (and then from, litterally moments later) Duration here are completely useless
        let location = Duration::from_secs_f32(
            self.player_state.pos as f32 / self.player_state.sample_rate as f32 / 2.0,
        );
        let total = Duration::from_secs_f32(self.player_state.total);

        let play_pause = match self.player_state.playing {
            audio::Playing::Playing => {
                iced::widget::button(iced::widget::text("pause")).on_press(Message::Pause)
            }
            audio::Playing::Paused => {
                iced::widget::button(iced::widget::text("play")).on_press(Message::Resume)
            }
        };

        iced::widget::row!(
            play_pause,
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
                    iced::widget::Image::new(artwork.as_ref().clone()).height(Length::Fixed(40.0)),
                )
            } else {
                iced::widget::container(iced::widget::text(""))
            },
            queue,
            iced::widget::button(iced::widget::text("skip")).on_press(Message::Skip),
        )
        .align_items(iced::Alignment::Center)
        .spacing(20)
        .into()
    }

    pub(crate) fn set_player_state(&mut self, state: audio::PlayerState) {
        self.player_state = state;
    }
}
