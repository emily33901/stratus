use super::{app::Message, song_list::SongList};
use crate::model;
use iced::widget;

use iced::widget::text;
use iced::Element;
use std::sync::Arc;

pub struct UserPage {
    user: Arc<model::User>,
    store: Arc<model::Store>,
    pub song_list: Option<SongList>,
}

impl UserPage {
    pub fn new(user: Arc<model::User>, store: &Arc<model::Store>) -> Self {
        Self {
            user,
            store: store.clone(),
            song_list: None,
        }
    }

    pub fn view(&self) -> Element<Message> {
        let mut column = widget::column!().spacing(20);

        let user_avatar: Element<Message> = if let Some(avatar) = &self.user.avatar {
            iced::widget::image::Image::new(avatar.as_ref().clone())
                .width(iced::Length::Fixed(100.0))
                .into()
        } else {
            text("").into()
        };

        column = column.push(
            widget::row!()
                .push(user_avatar)
                .push(text(format!("{}", self.user.username))),
        );

        if let Some(page) = &self.song_list {
            column = column.push(page.view());
        }

        column.into()
    }

    pub fn update_songs(&mut self, songs: Arc<model::Playlist>) {
        self.song_list = Some(SongList::new(songs))
    }
}
