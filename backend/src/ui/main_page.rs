use iced::{Column, Element, Text};

use super::{app::Message, App};

pub trait MainPage {
    fn get_main_page(&mut self) -> Element<Message>;
}

impl MainPage for App {
    fn get_main_page(&mut self) -> Element<Message> {
        Text::new("Main page").into()
    }
}
