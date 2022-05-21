use std::{collections::HashMap, sync::Arc};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use iced::{
    pure::{column, Element, Widget},
    Command,
};

use crate::sc;

use super::{
    app::Message,
    cache::{ImageCache, UserCache},
    song::Song,
};

#[derive(Default)]
struct SongHolder {
    song: Option<Song>,
    display: bool,
}

pub struct SongList {
    songs: HashMap<sc::OwnedId, SongHolder>,
    playlist: sc::Playlist,
}

impl SongList {
    pub fn new(playlist: sc::Playlist) -> Self {
        Self {
            songs: Default::default(),
            playlist,
        }
    }

    pub fn view(&self) -> Element<Message> {
        let column = column();

        for song in self
            .songs
            .values()
            .filter(|song| song.display)
            .filter_map(|song| song.song.as_ref())
        {
            column = column.push(song.view())
        }

        column.into()
    }

    pub fn update_filter(&mut self, str: &str) {
        let matcher = SkimMatcherV2::default();

        if str.len() < 2 {
            let _ = self
                .songs
                .values_mut()
                .map(|x| x.display = true)
                .collect::<Vec<_>>();
        } else {
            for song in self.songs.values_mut() {
                song.display = song
                    .song
                    .as_ref()
                    .map(|song| {
                        matcher.fuzzy_match(song.title(), str).is_some()
                            || song
                                .username()
                                .and_then(|username| matcher.fuzzy_match(username, str))
                                .is_some()
                    })
                    .unwrap_or_default();
            }
        }
    }

    pub fn song_loaded(
        &mut self,
        song: &sc::Song,
        image_cache: Arc<ImageCache>,
        user_cache: Arc<UserCache>,
    ) -> Command<Message> {
        if let Some(mut holder) = self.songs.get_mut(&sc::OwnedId::Id(song.object.id)) {
            *holder = SongHolder {
                song: Some(Song::new(song.clone(), image_cache.clone())),
                display: true,
            };
        }

        let object = song.user.clone();
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
        image_cache: Arc<ImageCache>,
    ) -> Command<Message> {
        for song in self
            .songs
            .values_mut()
            .filter_map(|holder| holder.song.as_mut())
        {
            if song.user_id() == user.object.id {
                song.user = Some(user.clone());
            }
        }

        Command::none()
    }
}
