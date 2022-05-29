use std::{collections::HashMap, sync::Arc};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use iced::{
    pure::{column, Element},
    Command,
};
use log::info;

use crate::sc;

use super::{
    app::Message,
    cache::{ImageCache, UserCache},
    song::Song,
};

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum Display {
    Show,
    Hidden,
}

impl Default for Display {
    fn default() -> Self {
        Self::Show
    }
}

#[derive(Default)]
pub struct SongHolder {
    song: Option<Song>,
    display: Display,
}

pub struct SongList {
    song_list: HashMap<sc::OwnedId, SongHolder>,
    playlist: sc::Playlist,
}

impl SongList {
    // TODO(emily): Needs to return Command aswell
    // or the message match in App needs to load the songs of the playlist
    pub fn new(playlist: sc::Playlist) -> Self {
        Self {
            song_list: Default::default(),
            playlist,
        }
    }

    pub fn view(&self) -> Element<Message> {
        let mut column = column();

        for song in self
            .song_list
            .values()
            .filter(|song| song.display == Display::Show)
            .filter_map(|song| song.song.as_ref())
        {
            column = column.push(song.view())
        }

        column.spacing(20).into()
    }

    pub fn update_filter(&mut self, str: &str) -> Command<Message> {
        let matcher = SkimMatcherV2::default();

        let str = str.to_owned();

        info!("blocking");
        let song_list: HashMap<sc::OwnedId, (Option<String>, Option<String>)> = self
            .song_list
            .iter()
            .map(|(k, v)| (k, (&v.song)))
            .map(|(k, song)| {
                (
                    k.clone(),
                    (
                        song.as_ref().map(|song| song.title().to_owned()),
                        song.as_ref()
                            .and_then(|song| song.username().map(|s| s.to_owned())),
                    ),
                )
            })
            .collect();
        info!("Not blocking");

        if str.len() < 2 {
            Command::perform(
                async move {
                    song_list
                        .iter()
                        .map(|(k, v)| (k.clone(), Display::Show))
                        .collect()
                },
                Message::SongListFilterComputed,
            )
        } else {
            Command::perform(
                async move {
                    song_list
                        .iter()
                        .map(|(k, (title, username))| {
                            (
                                k.clone(),
                                title
                                    .as_ref()
                                    .and_then(|title| matcher.fuzzy_match(title, &str))
                                    .or_else(|| {
                                        username.as_ref().and_then(|username| {
                                            matcher.fuzzy_match(username, &str)
                                        })
                                    })
                                    .map(|_| Display::Show)
                                    .unwrap_or(Display::Hidden),
                            )
                        })
                        .collect()
                },
                Message::SongListFilterComputed,
            )
        }
    }

    pub fn song_loaded(
        &mut self,
        song: &sc::Song,
        image_cache: &Arc<ImageCache>,
        user_cache: &Arc<UserCache>,
    ) -> Command<Message> {
        self.song_list.insert(
            sc::OwnedId::Id(song.object.id),
            SongHolder {
                song: Some(Song::new(song.clone(), image_cache.clone())),
                display: Display::Show,
            },
        );

        let object = song.user.clone();
        let user_cache = user_cache.clone();
        Command::perform(
            async move {
                user_cache.try_get(&object);
            },
            Message::None,
        )
    }

    pub fn user_loaded(
        &mut self,
        user: &sc::User,
        _image_cache: &Arc<ImageCache>,
    ) -> Command<Message> {
        for song in self
            .song_list
            .values_mut()
            .filter_map(|holder| holder.song.as_mut())
        {
            if song.user_id() == user.object.id {
                song.user = Some(user.clone());
            }
        }

        Command::none()
    }

    pub fn models(&self) -> impl Iterator<Item = &'_ Song> {
        self.song_list.values().filter_map(|h| h.song.as_ref())
    }

    pub fn playlist(&self) -> &sc::Playlist {
        &self.playlist
    }
    pub fn title(&self) -> &str {
        &self.playlist.title
    }

    pub(crate) fn filter_computed(
        &mut self,
        computed: &HashMap<sc::OwnedId, Display>,
    ) -> Command<Message> {
        info!("updating display");
        for (k, v) in computed {
            self.song_list
                .get_mut(k)
                .map(|holder| holder.display = v.clone());
        }

        info!("done updating display");

        Command::none()
    }
}
