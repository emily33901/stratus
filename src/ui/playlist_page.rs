use std::sync::Arc;

// use iced::pure::widget::{Button, Column, Container, Scrollable, Text, TextInput};
use iced::widget;
use iced::Command;
use iced::Element;

use crate::model;

use super::app::Message;
use super::song_list::SongList;

pub struct PlaylistPage {
    pub song_list: SongList,
    pub filter_text: String,
}

impl PlaylistPage {
    pub fn new(playlist: Arc<model::Playlist>) -> Self {
        Self {
            song_list: SongList::new(playlist),
            filter_text: Default::default(),
        }
    }

    pub fn view(&self) -> Element<Message> {
        let mut column = widget::column!().spacing(40);

        column = column
            .push(
                widget::text(format!(
                    "{} ({} tracks)",
                    self.song_list.title(),
                    self.song_list.playlist().songs.len()
                ))
                .size(40),
            )
            .push(widget::button(widget::text("Queue playlist")).on_press(Message::QueuePlaylist));

        // column = column.push(Text::new(playlist.).size(20)));
        // Filter by the filter string
        column = column.push(
            widget::text_input("Search...", &self.filter_text)
                .size(20)
                .on_input(Message::PlaylistFilterChange)
                .padding(10),
        );

        column = column.push(self.song_list.view());

        column.into()
    }

    pub fn filter_changed(&mut self, str: &str) -> Command<Message> {
        self.filter_text = str.to_string();

        self.song_list.update_filter(str)
    }

    pub fn songs(&self) -> impl Iterator<Item = &'_ Arc<model::Song>> + '_ {
        self.song_list.models().map(|s| s.song())
    }
}
