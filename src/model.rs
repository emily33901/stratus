use crate::sc;
use crate::{cache::Cache, sc::SoundCloud};
use eyre::{Result};
use parking_lot::Mutex;

use std::sync::Arc;
use tokio::sync::watch;

pub type Id = i64;

#[derive(Clone, Debug)]
pub struct Format {
    pub mime_type: String,
    pub protocol: String,
}

impl From<sc::api::model::Format> for Format {
    fn from(value: sc::api::model::Format) -> Self {
        Self {
            mime_type: value.mime_type,
            protocol: value.protocol,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Transcoding {
    pub url: String,
    pub format: Format,
}

impl From<sc::api::model::Transcoding> for Transcoding {
    fn from(value: sc::api::model::Transcoding) -> Self {
        Self {
            url: value.url,
            format: value.format.into(),
        }
    }
}

impl Transcoding {
    pub async fn resolve(&self) -> Result<String> {
        Ok(sc::api::hls_playlist(&self.url).await?)
    }
}

#[derive(Clone, Debug)]
pub struct Media {
    pub transcodings: Vec<Transcoding>,
}

impl From<sc::Media> for Media {
    fn from(value: sc::Media) -> Self {
        Self {
            transcodings: value.transcodings.into_iter().map(|v| v.into()).collect(),
        }
    }
}

#[derive(Debug)]
pub struct User {
    pub id: i64,
    pub permalink: Option<String>,
    pub uri: Option<String>,

    pub username: String,
    pub avatar_url: Option<String>,
    pub avatar: Option<Arc<iced::widget::image::Handle>>,
}

#[derive(Debug)]
pub struct Song {
    pub id: i64,
    pub permalink: Option<String>,
    pub uri: Option<String>,

    pub user: Arc<User>,
    pub artwork_url: Option<String>,
    pub artwork: Option<Arc<iced::widget::image::Handle>>,
    pub title: String,
    pub media: Media,
    pub full_duration: usize,
}

#[derive(Debug)]
pub struct Playlist {
    pub id: i64,
    pub permalink: Option<String>,
    pub uri: Option<String>,

    pub user: Arc<User>,
    pub artwork_url: Option<String>,
    pub artwork: Option<Arc<iced::widget::image::Handle>>,
    pub title: String,
    pub songs: Vec<Arc<Song>>,
}

pub struct Store {
    soundcloud: Arc<SoundCloud>,
    user_cache: Cache<Id, User>,
    song_cache: Cache<Id, Song>,
    playlist_cache: Cache<Id, Playlist>,
    likes_cache: Cache<Id, Playlist>,
    image_cache: Cache<String, iced::widget::image::Handle>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            soundcloud: Arc::new(SoundCloud::new()),
            user_cache: Default::default(),
            song_cache: Default::default(),
            playlist_cache: Default::default(),
            likes_cache: Default::default(),
            image_cache: Default::default(),
        }
    }

    pub async fn resolve_sc_user(&self, sc_user: sc::api::model::User) -> Result<Arc<User>> {
        let avatar = if let Some(url) = sc_user.avatar.as_ref() {
            Some(self.image(&url).await?)
        } else {
            None
        };

        Ok(Arc::new(User {
            id: sc_user.object.id,
            permalink: sc_user.object.url,
            uri: sc_user.object.uri,
            username: sc_user.username,
            avatar: avatar,
            avatar_url: sc_user.avatar,
        }))
    }

    pub async fn user(&self, id: &Id) -> Result<Arc<User>> {
        Ok(self
            .user_cache
            .get(id, async {
                let sc_user = self.soundcloud.user(sc::Id::Id(*id)).await?;
                self.resolve_sc_user(sc_user).await
            })
            .await?)
    }

    pub async fn likes(&self, id: &Id) -> Result<Arc<Playlist>> {
        Ok(self
            .likes_cache
            .get(id, async {
                let sc_playlist = self.soundcloud.likes(sc::Id::Id(*id)).await?;
                self.resolve_sc_playlist(sc_playlist).await
            })
            .await?)
    }

    pub async fn song(&self, id: &Id) -> Result<Arc<Song>> {
        Ok(self
            .song_cache
            .get(id, async {
                let sc_song = self.soundcloud.song(sc::Id::Id(*id)).await?;
                self.resolve_sc_song(sc_song).await
            })
            .await?)
    }

    async fn resolve_sc_song(&self, sc_song: sc::api::model::Song) -> Result<Arc<Song>> {
        let artwork = if let Some(url) = sc_song.artwork.as_ref() {
            Some(self.image(&url).await?)
        } else {
            None
        };

        Ok(Arc::new(Song {
            id: sc_song.object.id,
            permalink: sc_song.object.url,
            uri: sc_song.object.uri,
            user: self.resolve_sc_user(sc_song.user).await?,
            artwork: artwork,
            artwork_url: sc_song.artwork,
            title: sc_song.title,
            media: sc_song.media.into(),
            full_duration: sc_song.full_duration,
        }))
    }

    async fn resolve_sc_playlist(
        &self,
        sc_playlist: sc::api::model::Playlist,
    ) -> Result<Arc<Playlist>> {
        let artwork = if let Some(url) = sc_playlist.artwork.as_ref() {
            Some(self.image(&url).await?)
        } else {
            None
        };

        Ok(Arc::new(Playlist {
            id: sc_playlist.object.id,
            permalink: sc_playlist.object.url,
            uri: sc_playlist.object.uri,
            user: self.user(&sc_playlist.user.id).await?,
            artwork: artwork,
            artwork_url: sc_playlist.artwork,
            title: sc_playlist.title,
            songs: futures::future::join_all(sc_playlist.songs.into_iter().map(|song| {
                async move {
                    let id = song["id"].as_i64().unwrap();
                    if let Some(_) = song.get("artwork_url") {
                        // NOTE(emily): This Value is a real Song and we can just use it in place
                        self.song_cache
                            .write(
                                id,
                                self.resolve_sc_song(
                                    serde_json::from_value(song)
                                        .expect("Unable to deserialise sc Track"),
                                )
                                .await?,
                            )
                            .await
                    }
                    self.song(&id).await
                }
            }))
            .await
            .iter()
            .filter_map(|x| x.as_ref().ok())
            .cloned()
            .collect(),
        }))
    }

    pub async fn playlist(&self, id: &Id) -> Result<Arc<Playlist>> {
        Ok(self
            .playlist_cache
            .get(id, async {
                let sc_playlist = self.soundcloud.playlist(sc::Id::Id(*id)).await?;
                self.resolve_sc_playlist(sc_playlist).await
            })
            .await?)
    }

    pub async fn image(&self, url: &str) -> Result<Arc<iced::widget::image::Handle>> {
        Ok(self
            .image_cache
            .get(&url.to_owned(), async {
                self.soundcloud
                    .image(url)
                    .await
                    .map(|image| Arc::new(image))
            })
            .await?)
    }

    pub async fn resolve_url(&self, url: &str) -> Result<Id> {
        self.soundcloud.url(url).await.map(|r| r.id)
    }
}

#[derive(Clone, Debug)]
enum _Eventually<T> {
    NotAvailable(watch::Receiver<Arc<T>>),
    Available(Arc<T>),
}

impl<T> _Eventually<T> {
    pub fn new(sender: &watch::Sender<Arc<T>>) -> Self {
        Self::NotAvailable(sender.subscribe())
    }
}

#[derive(Debug)]
struct Eventually<T: Clone>(Mutex<_Eventually<T>>);

impl<T: Clone> Clone for Eventually<T> {
    fn clone(&self) -> Self {
        let inside = self.0.lock();
        Self(Mutex::new(inside.clone()))
    }
}

impl<T: Clone> Eventually<T> {
    pub fn maybe(&self) -> Option<Arc<T>> {
        let mut zelf = self.0.try_lock();
        match zelf.as_mut() {
            Some(garbage) => match &mut **garbage {
                _Eventually::NotAvailable(rx) => {
                    if let Ok(true) = rx.has_changed() {
                        let v = rx.borrow_and_update().clone();
                        **garbage = _Eventually::Available(v.clone());
                        Some(v)
                    } else {
                        None
                    }
                }
                _Eventually::Available(v) => Some(v.clone()),
            },
            None => None,
        }
    }
}
