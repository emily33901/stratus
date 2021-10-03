use iced::{Column, Container, Element, Length, Row, Scrollable, Text};

use super::{app::Message, App};

pub trait PlaylistPage {
    fn get_playlist_page(&mut self) -> Element<Message>;
}

impl PlaylistPage for App {
    fn get_playlist_page(&mut self) -> Element<Message> {
        let playlist = self.playlist.as_ref().unwrap();

        let mut column = Column::new().spacing(40);

        for song in &playlist.songs {
            column = column.push(
                {
                    if let Some(song) = self.song_cache.try_get(song) {
                        if let Some(image) = self.image_for_song(song) {
                            Row::new().push(image.width(Length::Units(100)))
                        } else {
                            Row::new()
                        }
                        .push(Text::new(&song.title))
                    } else {
                        Row::new().push(Text::new("Loading"))
                    }
                }
                .spacing(20),
            );
        }

        Scrollable::new(&mut self.scroll)
            .padding(40)
            .push(Container::new(column).width(Length::Fill).center_x())
            .into()
    }
}
