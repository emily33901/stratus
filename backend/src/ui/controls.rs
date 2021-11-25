use std::{ops::RangeInclusive, time};

use iced::{button, slider, Button, Row, Slider, Text};

use super::app::Message;

#[derive(Default)]
pub struct ControlsElement {
    play_button: button::State,
    pause_button: button::State,
    slider: slider::State,
}

impl ControlsElement {
    pub fn view(&mut self, location: time::Duration, total: time::Duration) -> Row<Message> {
        Row::new()
            .push(Button::new(&mut self.play_button, Text::new("play")).on_press(Message::Resume))
            .push(Button::new(&mut self.pause_button, Text::new("pause")).on_press(Message::Pause))
            .push(Text::new(format!("{:.1}", location.as_secs_f32())))
            .push(Slider::new(
                &mut self.slider,
                RangeInclusive::new(0.0, total.as_secs_f64()),
                location.as_secs_f64(),
                |_| Message::None,
            ))
            .push(Text::new(format!("{:.1}", total.as_secs_f32())))
            .align_items(iced::Align::Center)
    }
}
