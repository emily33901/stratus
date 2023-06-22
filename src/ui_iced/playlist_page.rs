use std::sync::Arc;

use iced::widget;
use iced::Command;
use iced::Element;

use crate::model;

use super::app::Message;
use super::song_list::SongList;

pub struct PlaylistPage {
    pub playlist: Arc<model::Playlist>,
    pub song_list: SongList,
    pub filter_text: String,
}

impl PlaylistPage {
    pub fn new(playlist: Arc<model::Playlist>) -> Self {
        Self {
            playlist: playlist.clone(),
            song_list: SongList::new(playlist),
            filter_text: Default::default(),
        }
    }

    pub fn view(&self) -> Element<Message> {
        let mut column = widget::column!(
            widget::row!(
                widget::text(format!(
                    "{} ({} tracks)",
                    self.song_list.title(),
                    self.song_list.playlist().songs.len()
                ))
                .size(40)
                .width(iced::Length::FillPortion(3)),
                widget::text_input("Search...", &self.filter_text)
                    .size(20)
                    .on_input(Message::PlaylistFilterChange)
                    .width(iced::Length::FillPortion(2)),
            )
            .padding(10),
            widget::button(widget::text("Queue playlist")).on_press(Message::QueuePlaylist),
        )
        .spacing(40);

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

    pub(crate) fn page_changed(&mut self, amount: isize) {
        self.song_list.page_changed(amount);
    }

    pub(crate) fn page_scroll(&mut self, amount: f32) {
        self.song_list.page_scroll(amount);
    }
}
