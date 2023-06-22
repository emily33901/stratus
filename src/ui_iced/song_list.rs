use std::{collections::HashMap, sync::Arc};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use iced::widget;
use iced::Command;
use iced::Element;

use crate::model;

use super::song;
use super::{app::Message, song::Song};

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum Display {
    Show(i64),
    Hidden,
}

impl Default for Display {
    fn default() -> Self {
        Self::Show(100)
    }
}

pub struct SongHolder {
    song: Song,
    display: Display,
}

pub struct SongList {
    song_list: HashMap<model::Id, SongHolder>,
    playlist: Arc<model::Playlist>,
    scroll_pos: f32,
}

impl SongList {
    pub fn new(playlist: Arc<model::Playlist>) -> Self {
        Self {
            song_list: HashMap::from_iter(playlist.songs.iter().map(|song| {
                (
                    song.id,
                    SongHolder {
                        song: Song::new(song.clone()),
                        display: Display::default(),
                    },
                )
            })),
            playlist,
            scroll_pos: 0.0,
        }
    }

    pub fn view(&self) -> Element<Message> {
        let mut column = widget::column!();

        // TODO(emily): Ideally, dont collect here, only to throw it away.
        // We collect right now to determine how many songs are actually going to be displayed
        // in total.
        let mut displayed_songs: Vec<&SongHolder> = self
            .song_list
            .values()
            .filter_map(|song| {
                if let Display::Hidden = song.display {
                    None
                } else {
                    Some(song)
                }
            })
            .collect();

        displayed_songs.sort_by_cached_key(|song| {
            // Safety: All Display::Hidden songs are removed beforehand
            let Display::Show(score) = song.display else { unsafe { std::hint::unreachable_unchecked()} };
            0 - score
        });

        let total_len = displayed_songs.len() as f32;

        for song in displayed_songs.into_iter().enumerate().map(|(i, song)| {
            let song_pos = i as f32 / total_len as f32;
            // TODO(emily): This '10' needs to have some relation to how many things
            // can actually fit on the screen.
            // for now just scale with how many things exist in the list
            if (song_pos - self.scroll_pos).abs() < (10 as f32 / total_len as f32) {
                Some(song)
            } else {
                None
            }
        }) {
            match song {
                Some(song) => column = column.push(song.song.view()),
                // TODO(emily): Get this height from the song element
                None => column = column.push(widget::column!().height(150.0)),
            };
        }

        widget::scrollable(column.spacing(20))
            .on_scroll(|ro| Message::PageScroll(ro.relative_offset().y))
            .into()
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

        if str.len() < 1 {
            Command::perform(
                async move {
                    song_list
                        .iter()
                        .map(|(k, _v)| (k.clone(), Display::default()))
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
                                    Display::Show(
                                        title_score.unwrap_or_default()
                                            + username_score.unwrap_or_default(),
                                    )
                                }
                            })
                        })
                        .collect()
                },
                Message::SongListFilterComputed,
            )
        }
    }

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

    pub(crate) fn page_changed(&mut self, amount: isize) {
        // let mut new_amount = self.cur_page as isize + amount;
        // if new_amount < 0 {
        // new_amount = 0;
        // }

        // self.cur_page = new_amount as usize;
    }

    pub(crate) fn page_scroll(&mut self, amount: f32) {
        self.scroll_pos = amount;
        // do something
    }
}
