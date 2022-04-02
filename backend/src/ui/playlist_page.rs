use std::{collections::VecDeque, sync::Arc};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use iced::{Column, Command, Container, Element, Length, Row, Scrollable, Text, TextInput};

use super::{app::Message, cache::ImageCache, song::Song};
use crate::sc;

#[derive(Default)]
struct SongHolder {
    song: Option<Song>,
    display: bool,
}
pub struct PlaylistPage {
    // Objects that this playlist wants
    pub playlist: sc::Playlist,
    // Built UI models of those objects
    songs: Vec<SongHolder>,
    scroll: iced::scrollable::State,
    filter: iced::text_input::State,
    pub filter_text: String,
}

impl PlaylistPage {
    pub fn new(playlist: sc::Playlist) -> Self {
        let _len = playlist.songs.len();
        let mut songs = vec![];
        songs.resize_with(playlist.songs.len(), || Default::default());

        Self {
            playlist,
            songs,
            scroll: Default::default(),
            filter: Default::default(),
            filter_text: Default::default(),
        }
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
        // Filter by the filter string
        column = column.push(
            TextInput::new(
                &mut self.filter,
                "Fuzzy search...",
                &self.filter_text,
                Message::PlaylistFilterChange,
            )
            .style(crate::ui::style::Theme::Dark),
        );

        for song in self
            .songs
            .iter_mut()
            .filter(|song| song.display)
            .filter_map(|song| song.song.as_mut())
        {
            column = column.push(song.view())
        }

        Scrollable::new(&mut self.scroll)
            .padding(40)
            .push(Container::new(column).width(Length::Fill).center_x())
            .into()
    }

    pub fn filter_changed(&mut self, str: &str) -> Command<Message> {
        self.filter_text = str.to_string();
        // let filter = &self.filter_text;
        let matcher = SkimMatcherV2::default();

        if str.len() == 0 {
            let _ = self
                .songs
                .iter_mut()
                .map(|x| x.display = true)
                .collect::<Vec<_>>();
        } else {
            for song in self.songs.iter_mut() {
                song.display = song
                    .song
                    .as_ref()
                    .and_then(|song| matcher.fuzzy_match(song.title(), str))
                    .map(|_| true)
                    .unwrap_or_default();
            }
        }

        Command::none()
    }

    pub fn song_loaded(
        &mut self,
        song: &sc::Song,
        image_cache: Arc<ImageCache>,
    ) -> Command<Message> {
        // TODO(emily): probably want a hashmap here
        for (i, object) in self.playlist.songs.iter().enumerate() {
            if object.id == song.object.id {
                self.songs[i] = SongHolder {
                    song: Some(Song::new(song.clone(), image_cache.clone())),
                    display: true,
                }
            }
        }

        Command::none()
    }
}
