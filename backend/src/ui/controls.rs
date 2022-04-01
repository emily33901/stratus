use std::{collections::VecDeque, ops::RangeInclusive, time};

use iced::{button, pick_list, slider, Button, Row, Slider, Text, Tooltip};

use super::app::Message;

#[derive(Default)]
pub struct ControlsElement {
    play_button: button::State,
    pause_button: button::State,
    skip_button: button::State,
    queue_state: pick_list::State<audio::SongId>,
    pub(crate) queue: VecDeque<audio::SongId>,
    slider: slider::State,
}

impl ControlsElement {
    pub fn view(&mut self, location: time::Duration, total: time::Duration) -> Row<Message> {
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
                        .map(|x| *x)
                        .collect::<Vec<audio::SongId>>(),
                    None,
                    |x| Message::None,
                )
                .style(crate::ui::style::Theme::Dark),
            )
            .align_items(iced::Align::Center)
    }
}
