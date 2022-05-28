use std::{collections::VecDeque, ops::RangeInclusive, sync::Arc, time};

// use iced::pure::widget::{button, pick_list, slider, Button, Row, Slider, Text, Tooltip};
use iced::pure::{Element};
use tokio::sync::watch;

use super::{app::Message, cache::SongCache};

use ellipse::Ellipse;

pub struct ControlsElement {
    cur_track_rx: watch::Receiver<Option<audio::SongId>>,
    pub(crate) queue: VecDeque<audio::SongId>,
    song_cache: Arc<SongCache>,
}

impl ControlsElement {
    pub fn new(
        song_cache: Arc<SongCache>,
        cur_track_rx: watch::Receiver<Option<audio::SongId>>,
    ) -> Self {
        Self {
            song_cache,
            cur_track_rx,
            queue: Default::default(),
        }
    }

    pub fn view(&self, location: time::Duration, total: time::Duration) -> Element<Message> {
        let song_cache = self.song_cache.clone();

        let options = self
            .queue
            .iter()
            .map(|id| {
                song_cache.try_get(&crate::sc::Object {
                    id: *id,
                    kind: "track".into(),
                    ..Default::default()
                })
            })
            .filter_map(|song| song.map(|song| song.title.as_str().truncate_ellipse(30).into()))
            .collect::<Vec<String>>();

        let selected = self
            .cur_track_rx
            .borrow()
            .and_then(|id| {
                song_cache.try_get(&crate::sc::Object {
                    id,
                    kind: "track".into(),
                    ..Default::default()
                })
            })
            .map(|x| x.title.as_str().truncate_ellipse(30).into());

        let queue = iced::pure::container(iced::pure::pick_list(options, selected, |_x| {
            Message::none()
        }))
        .style(crate::ui::style::Theme::Dark);

        iced::pure::row()
            .push(
                iced::pure::button(iced::pure::text("play"))
                    .on_press(Message::Resume)
                    .style(crate::ui::style::Theme::Dark),
            )
            .push(
                iced::pure::button(iced::pure::text("pause"))
                    .on_press(Message::Pause)
                    .style(crate::ui::style::Theme::Dark),
            )
            .push(
                iced::pure::button(iced::pure::text("skip"))
                    .on_press(Message::Skip)
                    .style(crate::ui::style::Theme::Dark),
            )
            .push(iced::pure::text(format!("{:.1}", location.as_secs_f32())))
            .push(
                iced::pure::slider(
                    RangeInclusive::new(0.0, total.as_secs_f64()),
                    location.as_secs_f64(),
                    |_| Message::None(()),
                )
                .style(crate::ui::style::Theme::Dark),
            )
            .push(iced::pure::text(format!("{:.1}", total.as_secs_f32())))
            .push(queue)
            .align_items(iced::Alignment::Center)
            .spacing(20)
            .padding(iced::Padding::new(20))
            .into()
    }
}
