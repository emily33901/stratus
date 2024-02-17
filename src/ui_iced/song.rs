use std::sync::Arc;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use iced::widget;
use iced::Element;
use iced::Length;
use once_cell::sync::OnceCell;

use crate::model;

use super::app::Message;

use iced::widget::Component;

static MATCHER: OnceCell<SkimMatcherV2> = OnceCell::new();

#[derive(Clone)]
pub struct Song {
    song: Arc<model::Song>,
}

impl Song {
    pub fn view(&self) -> Element<Message> {
        {
            if let Some(image) = &self.song.artwork {
                widget::row!(
                    widget::image::Image::new(image.as_ref().clone()).width(Length::Fixed(150.0))
                )
            } else {
                widget::row!()
            }
            .push(
                widget::row!()
                    .push(
                        widget::column!()
                            .push(widget::text(&self.song.title))
                            .push(
                                widget::button(widget::text(self.song.user.username.clone()))
                                    .on_press(Message::UserClicked(self.song.user.clone())),
                            )
                            .spacing(20)
                            .width(Length::Shrink),
                    )
                    .width(Length::Fill)
                    .spacing(20)
                    .push(
                        widget::button(widget::text("Add to queue"))
                            .on_press(Message::SongQueue(self.song.clone())),
                    ),
            )
        }
        .spacing(20)
        .into()
    }

    pub fn model(&self) -> &Arc<model::Song> {
        &self.song
    }

    pub fn title(&self) -> &str {
        &self.song.title
    }

    pub fn user_id(&self) -> model::Id {
        self.song.user.id
    }

    pub fn song(&self) -> &Arc<model::Song> {
        &self.song
    }

    pub fn username(&self) -> &str {
        self.song.user.username.as_ref()
    }

    pub fn match_score(&self, pattern: &str) -> Option<i64> {
        let matcher = MATCHER.get_or_init(|| SkimMatcherV2::default());
        let title_score = matcher.fuzzy_match(&self.song.title, pattern);
        let username_score = matcher.fuzzy_match(&self.song.user.username, pattern);
        if title_score.is_none() && username_score.is_none() {
            None
        } else {
            Some(title_score.unwrap_or_default() + username_score.unwrap_or_default())
        }
    }
}

impl Song {
    pub fn new(song: Arc<model::Song>) -> Self {
        Self { song }
    }
}
