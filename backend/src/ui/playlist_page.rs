use std::sync::Arc;

use iced::{Column, Command, Container, Element, Length, Row, Scrollable, Text};

use super::{
    app::Message,
    cache::{Cache, ImageCache},
    song::Song,
    App,
};
use crate::sc;

pub struct PlaylistPage {
    // Objects that this playlist wants
    pub playlist: sc::Playlist,
    // Built UI models of those objects
    songs: Vec<Option<Song>>,
    pub scroll: iced::scrollable::State,
}

impl PlaylistPage {
    pub fn new(playlist: sc::Playlist) -> Self {
        let len = playlist.songs.len();
        let mut songs = vec![];
        songs.resize_with(playlist.songs.len(), || None);
        let zelf = Self {
            playlist,
            songs,
            scroll: Default::default(),
        };

        zelf
    }

    pub fn view(&mut self) -> Element<Message> {
        let mut column = Column::new().spacing(40);

        column = column.push(
            Text::new(format!(
                "{} ({} tracks)",
                self.playlist.title.clone(),
                self.playlist.songs.len()
            ))
            .size(40),
        );
        // column = column.push(Text::new(playlist.).size(20)));

        for song in self.songs.iter_mut().filter_map(|song| song.as_mut()) {
            column = column.push(song.view())
        }

        Scrollable::new(&mut self.scroll)
            .padding(40)
            .push(Container::new(column).width(Length::Fill).center_x())
            .into()
    }

    pub fn song_loaded(
        &mut self,
        song: &sc::Song,
        image_cache: Arc<ImageCache>,
    ) -> Command<Message> {
        for (i, object) in self.playlist.songs.iter().enumerate() {
            if object.id == song.object.id {
                self.songs[i] = Some(Song::new(song.clone(), image_cache.clone()));
            }
        }

        Command::none()
    }
}
