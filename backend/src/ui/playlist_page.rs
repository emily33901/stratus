use std::{collections::VecDeque, sync::Arc};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
// use iced::pure::widget::{Button, Column, Container, Scrollable, Text, TextInput};
use iced::pure::Element;
use iced::pure::{button, column, container, scrollable, text, text_input};
use iced::{Command, Length};

use super::song_list::SongList;
use super::{
    app::Message,
    cache::{ImageCache, UserCache},
    song::Song,
};
use crate::sc::{self, api::model};

pub struct PlaylistPage {
    pub song_list: SongList,
    pub filter_text: String,
}

impl PlaylistPage {
    pub fn new(playlist: sc::Playlist) -> Self {
        Self {
            song_list: SongList::new(playlist),
            filter_text: Default::default(),
        }
    }

    pub fn view(&self) -> Element<Message> {
        let mut column = column().spacing(40);

        column = column
            .push(
                text(format!(
                    "{} ({} tracks)",
                    self.song_list.title(),
                    self.song_list.playlist().songs.len()
                ))
                .size(40),
            )
            .push(
                button(text("Queue playlist"))
                    .on_press(Message::QueuePlaylist)
                    .style(crate::ui::style::Theme::Dark),
            );

        // column = column.push(Text::new(playlist.).size(20)));
        // Filter by the filter string
        column = column.push(
            text_input(
                "Search...",
                &self.filter_text,
                Message::PlaylistFilterChange,
            )
            .style(crate::ui::style::Theme::Dark)
            .size(20)
            .padding(10),
        );

        column = column.push(self.song_list.view());

        column.into()
    }

    pub fn filter_changed(&mut self, str: &str) -> Command<Message> {
        self.filter_text = str.to_string();

        self.song_list.update_filter(str);

        Command::none()
    }

    pub fn songs(&self) -> impl Iterator<Item = &'_ model::Song> + '_ {
        self.song_list.models().map(|s| s.song())
    }
}
