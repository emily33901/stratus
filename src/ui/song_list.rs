use std::{collections::HashMap, sync::Arc};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use iced::widget;
use iced::Command;
use iced::Element;

use crate::model;

use super::{app::Message, song::Song};

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

pub struct SongHolder {
    song: Song,
    display: Display,
}

pub struct SongList {
    song_list: HashMap<model::Id, SongHolder>,
    playlist: Arc<model::Playlist>,
}

impl SongList {
    pub fn new(playlist: Arc<model::Playlist>) -> Self {
        Self {
            song_list: HashMap::from_iter(playlist.songs.iter().map(|song| {
                (
                    song.id,
                    SongHolder {
                        song: Song::new(song.clone()),
                        display: Display::Show,
                    },
                )
            })),
            playlist,
        }
    }

    pub fn view(&self) -> Element<Message> {
        let mut column = widget::column!();

        for song in self
            .song_list
            .values()
            .filter(|song| song.display == Display::Show)
        {
            column = column.push(song.song.view())
        }

        column.spacing(20).into()
    }

    pub fn update_filter(&mut self, str: &str) -> Command<Message> {
        let matcher = SkimMatcherV2::default();

        let str = str.to_owned();

        let song_list: HashMap<model::Id, (String, String)> = self
            .song_list
            .iter()
            .map(|(k, v)| (k, (&v.song)))
            .map(|(k, song)| {
                (
                    k.clone(),
                    (song.title().to_owned(), song.username().to_owned()),
                )
            })
            .collect();

        if str.len() < 2 {
            Command::perform(
                async move {
                    song_list
                        .iter()
                        .map(|(k, _v)| (k.clone(), Display::Show))
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
                            (k.clone(), {
                                let title_score = matcher.fuzzy_match(title, &str);
                                let username_score = matcher.fuzzy_match(username, &str);
                                if title_score.is_none() && username_score.is_none() {
                                    Display::Hidden
                                } else {
                                    Display::Show
                                }
                            })
                        })
                        .collect()
                },
                Message::SongListFilterComputed,
            )
        }
    }

    // pub fn song_loaded(
    //     &mut self,
    //     song: Arc<model::Song>,
    // ) -> Command<Message> {
    //     self.song_list.insert(
    //         song.id,
    //         SongHolder {
    //             song: Some(Song::new(song.clone(), image_cache.clone())),
    //             display: Display::Show,
    //         },
    //     );

    //     let object = song.user.clone();
    //     let user_cache = user_cache.clone();
    //     Command::perform(
    //         async move {
    //             user_cache.try_get(&object);
    //         },
    //         Message::None,
    //     )
    // }

    // pub fn user_loaded(
    //     &mut self,
    //     user: &Arc<model::User>,
    //     _image_cache: &Arc<ImageCache>,
    // ) -> Command<Message> {
    //     for song in self
    //         .song_list
    //         .values_mut()
    //         .filter_map(|holder| holder.song.as_mut())
    //     {
    //         if song.user_id() == user.object.id {
    //             song.user = Some(user.clone());
    //         }
    //     }

    //     Command::none()
    // }

    pub fn models(&self) -> impl Iterator<Item = &'_ Song> {
        self.song_list.values().map(|h| &h.song)
    }

    pub fn playlist(&self) -> &Arc<model::Playlist> {
        &self.playlist
    }

    pub fn title(&self) -> &str {
        self.playlist.title.as_str()
    }

    pub(crate) fn filter_computed(
        &mut self,
        computed: &HashMap<model::Id, Display>,
    ) -> Command<Message> {
        for (k, v) in computed {
            self.song_list
                .get_mut(k)
                .map(|holder| holder.display = v.clone());
        }

        Command::none()
    }
}
