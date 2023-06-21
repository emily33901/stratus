use super::app::Message;
use crate::model::{self};
use ellipse::Ellipse;
use iced::{widget, Command, Element, Length};
use std::{ops::RangeInclusive, sync::Arc, time};

pub struct ControlsElement {
    options: Vec<String>,
    cur_song: Option<Arc<model::Song>>,
    player_state: audio::PlayerState,
    volume: f32,
}

impl ControlsElement {
    pub fn new() -> Self {
        Self {
            cur_song: None,
            options: vec![],
            player_state: Default::default(),
            volume: 100.0,
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
        let queue = widget::container(widget::pick_list(
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
                widget::button(widget::text("pause")).on_press(Message::Pause)
            }
            audio::Playing::Paused => {
                widget::button(widget::text("play")).on_press(Message::Resume)
            }
        };

        widget::row!(
            play_pause.width(Length::Shrink),
            widget::row!(
                widget::text(format!("{:.1}", location.as_secs_f32())),
                widget::slider(
                    RangeInclusive::new(0.0, total.as_secs_f64()),
                    location.as_secs_f64(),
                    |_| Message::None(()),
                ),
                iced::widget::text(format!("{:.1}", total.as_secs_f32())),
            )
            .align_items(iced::Alignment::Center)
            .spacing(20)
            .width(Length::FillPortion(5)),
            widget::button(widget::text("skip"))
                .on_press(Message::Skip)
                .width(Length::Shrink),
            if let Some(artwork) = self
                .cur_song
                .as_ref()
                .and_then(|s| s.artwork.clone())
                .as_ref()
            {
                widget::container(
                    widget::Image::new(artwork.as_ref().clone()).height(Length::Fixed(40.0)),
                )
            } else {
                widget::container(widget::text(""))
            },
            queue.width(Length::Shrink),
            widget::slider(RangeInclusive::new(0.0, 100.0), self.volume, |v| {
                // TODO(emily): This is so stupid, but slider apparently works in increments of 1.0?
                // So we convert here to be in the correct range (0.0..1.0)
                Message::VolumeChange(v / 100.0)
            })
            .width(Length::FillPortion(1))
        )
        .align_items(iced::Alignment::Center)
        .spacing(20)
        .into()
    }

    pub(crate) fn set_player_state(&mut self, state: audio::PlayerState) {
        self.player_state = state;
    }

    pub(crate) fn volume_changed(&mut self, volume: f32) {
        // TODO(emily): See above TODO
        self.volume = volume * 100.0;
    }
}
