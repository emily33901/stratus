use futures::Future;
use iced::time;
use static_assertions::assert_impl_all;
use std::cell::RefCell;
use std::sync::Arc;
use tokio::sync::Mutex;

use async_trait::async_trait;
use eyre::eyre;
use eyre::Result;
use iced::image::Handle;
use iced::{self, button, executor, Application, Button, Column, Command, Image, Text};
use log::{info, warn};

use super::{cache::Cache, main_page::MainPage, playlist_page::PlaylistPage};
use crate::sc::{self, Id, SoundCloud};

pub enum Page {
    Main,
    Playlist,
}

impl Default for Page {
    fn default() -> Self {
        Page::Main
    }
}

#[derive(Default)]
pub struct App {
    page: Page,

    pub playlist: Option<sc::Playlist>,
    pub image_cache: Cache<String, Handle>,
    pub song_cache: Cache<sc::Object, sc::Song>,

    pub player: Arc<Mutex<Option<audio::HlsPlayer>>>,

    pub scroll: iced::scrollable::State,
}

impl App {
    pub fn image_for_song(&self, song: &sc::Song) -> Option<Image> {
        let url = song.artwork.as_ref()?.replace("-large", "-t500x500");

        let handle = self.image_cache.try_get(&url)?;
        Some(Image::new(handle.clone()))
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    None,
    PlaylistClicked(sc::Playlist),
    SongLoaded(sc::Song),
    ImageLoaded((String, Handle)),
}

struct Downloader {}
#[async_trait]
impl audio::Downloader for Downloader {
    async fn download(&self, url: &str) -> Result<Vec<u8>> {
        let client = reqwest::Client::new();
        let response = client.get(url).send().await?;
        Ok(response.bytes().await?.to_vec())
    }
}

impl Application for App {
    type Executor = executor::Default;

    type Message = Message;

    type Flags = ();

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (
            Self::default(),
            async {
                let playlist =
                    SoundCloud::playlist(Id::Url("https://soundcloud.com/f1ssi0n/sets/aaa"))
                        .await
                        .unwrap();
                SoundCloud::frame();
                Message::PlaylistClicked(playlist.clone())
            }
            .into(),
        )
    }

    fn title(&self) -> String {
        "stratus".into()
    }

    fn update(
        &mut self,
        message: Self::Message,
        clipboard: &mut iced::Clipboard,
    ) -> Command<Self::Message> {
        match &message {
            Message::None => (),
            message => info!("{:?}", message),
        }

        let msg_command = match message {
            Message::PlaylistClicked(playlist) => {
                let playlist2 = playlist.clone();
                self.playlist = Some(playlist.clone());
                self.page = Page::Playlist;

                Command::batch(playlist2.songs.into_iter().map(|song| {
                    async move {
                        song.preload().await;
                        Message::None
                    }
                    .into()
                }))
            }
            Message::ImageLoaded((url, handle)) => {
                info!("Image loaded: {}", url);
                self.image_cache.write(url, handle);
                Command::none()
            }
            Message::SongLoaded(song) => self.song_loaded(&song),
            _ => Command::none(),
        };

        // Queue loading images that need it
        let image_loads = Command::batch(self.image_cache.needs_loading().into_iter().map(|url| {
            async {
                info!("Loading image: {}", url);
                match reqwest::get(&url).await {
                    Ok(response) => {
                        let bytes = response.bytes().await.unwrap().to_vec();
                        Message::ImageLoaded((url, Handle::from_memory(bytes)))
                    }
                    Err(err) => {
                        warn!("Failed to get {}: {}", &url, err);
                        Message::None
                    }
                }
            }
            .into()
        }));

        let song_loads =
            Command::batch(self.song_cache.needs_loading().into_iter().map(|object| {
                async move {
                    info!("Loading song: {}", object.id);
                    match SoundCloud::song(Id::Id(object.id)).await {
                        Ok(song) => Message::SongLoaded(song),
                        Err(err) => {
                            warn!("Failed to get {}: {}", object.id, err);
                            Message::None
                        }
                    }
                }
                .into()
            }));

        Command::batch([msg_command, image_loads, song_loads])
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        time::every(std::time::Duration::from_millis(500)).map(|_| Message::None)
    }

    fn view(&mut self) -> iced::Element<Self::Message> {
        Column::new()
            .push(match self.page {
                Page::Main => self.get_main_page(),
                Page::Playlist => self.get_playlist_page(),
            })
            .into()
    }
}

impl App {
    fn song_loaded(&mut self, song: &sc::Song) -> iced::Command<Message> {
        info!("Song loaded: {}", song.title);
        self.song_cache.write(song.object.clone(), song.clone());

        for media in song.media.clone().transcodings {
            if media.format.mime_type == "audio/mpeg" {
                return async move {
                    if let Ok(m3u8) = media.resolve().await {
                        info!("{}", m3u8);

                        let player = audio::HlsPlayer::new(&m3u8, Box::new(Downloader {})).unwrap();
                        player.play();
                        let _ = player.download().await.unwrap();
                        player.play();
                        player.sink.detach();
                    }

                    Message::None
                }
                .into();
            }
        }

        Command::none()
    }
}
