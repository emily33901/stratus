use std::{sync::Arc, time};

use audio::HlsPlayer;
use iced::{button, Button, Element, Row, Text};
use parking_lot::Mutex;

use super::app::Message;

#[derive(Default)]
pub struct ControlsElement {
    play_button: button::State,
    pause_button: button::State,
}

impl ControlsElement {
    pub fn view(&mut self, location: time::Duration) -> Row<Message> {
        Row::new()
            .push(Button::new(&mut self.play_button, Text::new("play")).on_press(Message::Resume))
            .push(Button::new(&mut self.pause_button, Text::new("pause")).on_press(Message::Pause))
            .push(Text::new(format!("{:?}", location)))
            .into()
    }
}
