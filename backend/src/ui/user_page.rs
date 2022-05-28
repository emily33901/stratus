use std::sync::Arc;

use iced::pure::{row, text, Element};

use crate::sc;

use super::{app::Message, cache::ImageCache, song_list::SongList};

use iced::pure::{column};

pub struct UserPage {
    user: sc::User,
    image_cache: Arc<ImageCache>,
    pub song_list: Option<SongList>,
}

impl UserPage {
    pub fn new(user: sc::User, image_cache: &Arc<ImageCache>) -> Self {
        Self {
            user,
            image_cache: image_cache.clone(),
            song_list: None,
        }
    }

    pub fn view(&self) -> Element<Message> {
        let mut column = column().spacing(20);

        let user_avatar: Element<Message> =
            if let Some(avatar) = self.image_cache.image_for_user(&self.user) {
                avatar.width(iced::Length::Units(100)).into()
            } else {
                text("").into()
            };

        column = column.push(
            row()
                .push(user_avatar)
                .push(text(format!("{}", self.user.username))),
        );

        if let Some(page) = &self.song_list {
            column = column.push(page.view());
        }

        column.into()
    }

    pub fn update_songs(&mut self, songs: sc::Playlist) {
        self.song_list = Some(SongList::new(songs))
    }
}
