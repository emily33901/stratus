use super::app::Message;
use crate::model::{self};
use ellipse::Ellipse;
use iced::{widget, Command, Element, Length};
use std::{ops::RangeInclusive, sync::Arc, time};

fn format_duration(duration: &std::time::Duration) -> String {
    let secs = duration.as_secs();
    let hours = secs / 3600;
    let minutes = secs % 3600 / 60;
    let seconds = secs % 60;

    if hours == 0 {
        format!("{minutes}:{seconds:02}")
    } else {
        format!("{hours}:{minutes}:{seconds:02}")
    }
}

pub struct ControlsElement {
    options: Vec<String>,
    cur_song: Option<Arc<model::Song>>,
    player_state: audio::PlayerState,
    volume: f32,
    looping: audio::Looping,
}

impl ControlsElement {
    pub fn new() -> Self {
        Self {
            cur_song: None,
            options: vec![],
            player_state: Default::default(),
            volume: 100.0,
            looping: audio::Looping::LoopOne,
        }
    }

    pub fn queue_changed(&mut self, queue: &[Arc<model::Song>]) -> iced::Command<Message> {
        self.options = queue
            .iter()
            .map(|s| {
                format!(
                    "{} | {}",
                    s.user.username.as_str().truncate_ellipse(15),
                    s.title.as_str().truncate_ellipse(15)
                )
            })
            .collect();

        Command::none()
    }

    pub fn set_cur_song(&mut self, song: Option<Arc<model::Song>>) {
        self.cur_song = song;
    }

    pub fn view(&self) -> Element<Message> {
        use std::time::Duration;

        // TODO(emily): Conversion to (and then from, litterally moments later) Duration here are completely useless
        let location = Duration::from_secs_f32(
            self.player_state.pos as f32 / self.player_state.sample_rate as f32 / 2.0,
        );
        let total = Duration::from_secs_f32(self.player_state.total);

        let play_pause = match self.player_state.playing {
            audio::Playing::Playing => widget::button(widget::text("I I")).on_press(Message::Pause),
            audio::Playing::Paused => widget::button(widget::text(">")).on_press(Message::Resume),
        };

        let artwork = if let Some(artwork) = self
            .cur_song
            .as_ref()
            .and_then(|s| s.artwork.clone())
            .as_ref()
        {
            widget::container(widget::Image::new(artwork.as_ref().clone()))
        } else {
            widget::container(widget::row!())
        }
        .height(Length::Fixed(75.0))
        .width(Length::Fixed(75.0));

        let song_title_user = widget::container(widget::column!(
            widget::text(
                self.cur_song
                    .as_ref()
                    .map(|s| s.title.clone())
                    .unwrap_or_default()
            )
            .size(16),
            widget::text(
                self.cur_song
                    .as_ref()
                    .map(|s| s.user.username.clone())
                    .unwrap_or_default()
            )
            .size(13)
        ));

        let controls = widget::container(
            widget::column!(
                widget::row!(
                    widget::row!().width(Length::FillPortion(1)),
                    play_pause.width(Length::Shrink),
                    widget::button(widget::text(">>"))
                        .on_press(Message::Skip)
                        .width(Length::Shrink),
                    widget::button(widget::text(match self.looping {
                        audio::Looping::LoopOne => "loop1",
                        audio::Looping::Loop => "loop",
                        audio::Looping::None => "no loop",
                        _ => unreachable!(),
                    }))
                    .on_press(Message::LoopingChanged),
                    widget::row!().width(Length::FillPortion(1)),
                )
                .align_items(iced::Alignment::Center)
                .spacing(20)
                .width(Length::Fill),
                widget::row!(
                    widget::text(format!("{}", format_duration(&location))),
                    widget::slider(
                        RangeInclusive::new(0.0, total.as_secs_f64()),
                        location.as_secs_f64(),
                        |_| Message::None(()),
                    ),
                    iced::widget::text(format!("{}", format_duration(&total))),
                )
                .align_items(iced::Alignment::Center)
                .spacing(20)
            )
            .align_items(iced::Alignment::Center)
            .width(Length::Fill),
        )
        .width(Length::FillPortion(10));

        widget::row!(
            widget::row!(artwork, song_title_user)
                .spacing(20)
                .width(Length::FillPortion(3))
                .align_items(iced::Alignment::Center),
            controls,
            widget::container(widget::slider(
                RangeInclusive::new(0.0, 100.0),
                self.volume,
                |v| {
                    // TODO(emily): This is so stupid, but slider apparently works in increments of 1.0?
                    // So we convert here to be in the correct range (0.0..1.0)
                    Message::VolumeChange(v / 100.0)
                }
            ))
            .width(Length::FillPortion(1))
        )
        .height(Length::Fixed(75.0))
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

    pub(crate) fn rotate_looping(&mut self) -> audio::Looping {
        self.looping = match self.looping {
            audio::Looping::None => audio::Looping::LoopOne,
            audio::Looping::LoopOne => audio::Looping::Loop,
            audio::Looping::Loop => audio::Looping::None,
        };

        self.looping
    }
}
