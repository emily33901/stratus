use std::sync::Arc;

use iced::{Column, Command, Container, Element, Length, Row, Scrollable, Text};

use super::{
    app::Message,
    cache::{Cache, ImageCache},
    song::Song,
    App,
};
use crate::sc;

#[derive(Default)]
pub struct PlaylistPage {
    // Objects that this playlist wants
    pub playlist: Option<sc::Playlist>,
    // Built UI models of those objects
    songs: Vec<Song>,
    pub scroll: iced::scrollable::State,
}

impl PlaylistPage {
    pub fn view(&mut self, song_cache: &Cache<sc::Object, sc::Song>) -> Element<Message> {
        let mut column = Column::new().spacing(40);

        if let Some(playlist) = self.playlist.as_ref() {
            column = column.push(
                Text::new(format!(
                    "{} ({} tracks)",
                    playlist.title.clone(),
                    playlist.songs.len()
                ))
                .size(40),
            );
            // column = column.push(Text::new(playlist.).size(20)));
        }

        if self.songs.len() == 0 {
            if let Some(playlist) = self.playlist.as_ref() {
                playlist
                    .songs
                    .iter()
                    .map(|o| song_cache.try_get(o))
                    .for_each(drop);
            }
        }

        for song in &mut self.songs {
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
        if let Some(playlist) = self.playlist.as_ref() {
            for object in &playlist.songs {
                if object.id == song.object.id {
                    self.songs
                        .push(Song::new(song.clone(), image_cache.clone()));
                }
            }
        }

        Command::none()
    }
}
