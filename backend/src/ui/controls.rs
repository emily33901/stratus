use std::{collections::VecDeque, ops::RangeInclusive, sync::Arc, time};

use iced::{button, pick_list, slider, Button, Row, Slider, Text, Tooltip};
use tokio::sync::watch;

use super::{app::Message, cache::SongCache};

pub struct ControlsElement {
    play_button: button::State,
    pause_button: button::State,
    skip_button: button::State,
    cur_track_rx: watch::Receiver<Option<audio::SongId>>,
    queue_state: pick_list::State<String>,
    pub(crate) queue: VecDeque<audio::SongId>,
    slider: slider::State,
    song_cache: Arc<SongCache>,
}

impl ControlsElement {
    pub fn new(
        song_cache: Arc<SongCache>,
        cur_track_rx: watch::Receiver<Option<audio::SongId>>,
    ) -> Self {
        Self {
            song_cache,
            play_button: Default::default(),
            pause_button: Default::default(),
            skip_button: Default::default(),
            cur_track_rx,
            queue_state: Default::default(),
            queue: Default::default(),
            slider: Default::default(),
        }
    }

    pub fn view(&mut self, location: time::Duration, total: time::Duration) -> Row<Message> {
        use ellipse::Ellipse;
        let song_cache = self.song_cache.clone();
        Row::new()
            .push(
                Button::new(&mut self.play_button, Text::new("play"))
                    .on_press(Message::Resume)
                    .style(crate::ui::style::Theme::Dark),
            )
            .push(
                Button::new(&mut self.pause_button, Text::new("pause"))
                    .on_press(Message::Pause)
                    .style(crate::ui::style::Theme::Dark),
            )
            .push(
                Button::new(&mut self.skip_button, Text::new("skip"))
                    .on_press(Message::Skip)
                    .style(crate::ui::style::Theme::Dark),
            )
            .push(Text::new(format!("{:.1}", location.as_secs_f32())))
            .push(
                Slider::new(
                    &mut self.slider,
                    RangeInclusive::new(0.0, total.as_secs_f64()),
                    location.as_secs_f64(),
                    |_| Message::None,
                )
                .style(crate::ui::style::Theme::Dark),
            )
            .push(Text::new(format!("{:.1}", total.as_secs_f32())))
            .push(
                pick_list::PickList::new(
                    &mut self.queue_state,
                    self.queue
                        .iter()
                        .map(|id| {
                            song_cache.try_get(&crate::sc::Object {
                                id: *id,
                                kind: "track".into(),
                                ..Default::default()
                            })
                        })
                        .filter_map(|song| {
                            song.map(|song| song.title.as_str().truncate_ellipse(30).into())
                        })
                        .collect::<Vec<String>>(),
                    self.cur_track_rx
                        .borrow()
                        .and_then(|id| {
                            song_cache.try_get(&crate::sc::Object {
                                id: id,
                                kind: "track".into(),
                                ..Default::default()
                            })
                        })
                        .map(|x| x.title.as_str().truncate_ellipse(30).into()),
                    |x| Message::None,
                )
                .style(crate::ui::style::Theme::Dark),
            )
            .align_items(iced::Align::Center)
    }
}
