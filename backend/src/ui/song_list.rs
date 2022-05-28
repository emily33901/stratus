use std::{collections::HashMap, sync::Arc};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use iced::{
    pure::{column, Element, Widget},
    Command,
};
use log::info;

use crate::sc;

use super::{
    app::Message,
    cache::{ImageCache, UserCache},
    song::Song,
};

#[derive(Default)]
pub struct SongHolder {
    song: Option<Song>,
    display: bool,
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
            .filter(|song| song.display)
            .filter_map(|song| song.song.as_ref())
        {
            column = column.push(song.view())
        }

        column.spacing(20).into()
    }

    pub fn update_filter(&mut self, str: &str) {
        let matcher = SkimMatcherV2::default();

        if str.len() < 2 {
            let _ = self
                .song_list
                .values_mut()
                .map(|x| x.display = true)
                .collect::<Vec<_>>();
        } else {
            for song in self.song_list.values_mut() {
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
        image_cache: &Arc<ImageCache>,
        user_cache: &Arc<UserCache>,
    ) -> Command<Message> {
        self.song_list.insert(
            sc::OwnedId::Id(song.object.id),
            SongHolder {
                song: Some(Song::new(song.clone(), image_cache.clone())),
                display: true,
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
        image_cache: &Arc<ImageCache>,
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
}
