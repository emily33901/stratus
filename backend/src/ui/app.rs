use audio::HlsPlayer;
use futures::stream::BoxStream;
use iced::time;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

use async_trait::async_trait;

use eyre::Result;
use iced::image::Handle;
use iced::{self, executor, Application, Column, Command, Text};
use log::{info, warn};

use super::cache::{ImageCache, SongCache, UserCache};
use super::controls::ControlsElement;
use super::playlist_page::PlaylistPage;
use crate::sc::{self, Id, SoundCloud};

enum Page {
    Main,
    Playlist(PlaylistPage),
}

impl Default for Page {
    fn default() -> Self {
        Page::Main
    }
}

pub struct App {
    page: Page,

    playlist: Option<sc::Playlist>,
    image_cache: Arc<ImageCache>,
    song_cache: Arc<SongCache>,
    user_cache: Arc<UserCache>,

    player: Arc<Mutex<audio::HlsPlayer>>,
    current_time: Arc<AtomicUsize>,
    total_time: f32,
    queue: VecDeque<audio::TrackId>,

    scroll: iced::scrollable::State,
    controls: ControlsElement,
}

impl Default for App {
    fn default() -> Self {
        Self {
            page: Default::default(),
            playlist: Default::default(),
            image_cache: Default::default(),
            song_cache: Default::default(),
            user_cache: Default::default(),
            player: Arc::new(Mutex::new(HlsPlayer::new(Arc::new(Downloader::new())))),
            current_time: Default::default(),
            total_time: Default::default(),
            scroll: Default::default(),
            controls: Default::default(),
            queue: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    None,
    Tick,
    PlaylistClicked(sc::Playlist),
    SongLoaded(sc::Song),
    ImageLoaded((String, Handle)),
    SongQueue(sc::Song),
    Resume,
    Pause,
    Skip,
    QueueChanged(VecDeque<audio::TrackId>),
}

struct Downloader {
    client: reqwest::Client,
}

impl Downloader {
    fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl audio::Downloader for Downloader {
    async fn download(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.client.get(url).send().await?;
        Ok(response.bytes().await?.to_vec())
    }
}

impl Application for App {
    type Executor = executor::Default;

    type Message = Message;

    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (
            Self::default(),
            async {
                let playlist =
                    SoundCloud::playlist(Id::Url("https://soundcloud.com/f1ssi0n/sets/feel"))
                        .await
                        .unwrap();
                SoundCloud::frame();
                Message::PlaylistClicked(playlist)
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
        _clipboard: &mut iced::Clipboard,
    ) -> Command<Self::Message> {
        match &message {
            Message::None | Message::Tick => (),
            message => info!("{:?}", message),
        }

        let msg_command = match message {
            Message::PlaylistClicked(playlist) => {
                let playlist2 = playlist.clone();

                self.page = Page::Playlist(PlaylistPage::new(playlist.clone()));

                playlist
                    .songs
                    .iter()
                    .map(|o| self.song_cache.try_get(o))
                    .for_each(drop);

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
            Message::SongQueue(song) => self.queue_song(&song),
            Message::Resume => {
                let player = self.player.clone();
                async move {
                    let player = player.lock().await;
                    player.resume().await.unwrap();
                    Message::None
                }
                .into()
            }
            Message::Pause => {
                let player = self.player.clone();
                async move {
                    let player = player.lock().await;
                    player.pause().await.unwrap();
                    Message::None
                }
                .into()
            }
            Message::Skip => {
                let player = self.player.clone();
                async move {
                    let player = player.lock().await;
                    player.skip().await.unwrap();
                    Message::None
                }
                .into()
            }
            Message::QueueChanged(queue) => {
                self.queue = queue;
                async { Message::None }.into()
            }
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

        let current_time = self.current_time.clone();
        let player = self.player.clone();
        let update_pos = async move {
            current_time.store(player.lock().await.position(), Ordering::Release);
            Message::None
        }
        .into();

        self.total_time = self.player.blocking_lock().total();

        Command::batch([msg_command, image_loads, song_loads, update_pos])
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        iced::Subscription::batch([
            time::every(std::time::Duration::from_millis(500)).map(|_| Message::Tick),
            iced::Subscription::from_recipe(QueuedTrackRecipe {
                watch: self.player.blocking_lock().queued_watch(),
            }),
        ])
    }

    fn view(&mut self) -> iced::Element<Self::Message> {
        use iced::Element;
        Column::new()
            .push::<Element<Message>>(
                Column::new()
                    .push(match &mut self.page {
                        Page::Main => Text::new("Main page").into(),
                        Page::Playlist(playlist_page) => playlist_page.view(),
                    })
                    .height(iced::Length::FillPortion(1))
                    .into(),
            )
            .push(
                Column::new()
                    .push(
                        {
                            self.controls.view(
                                std::time::Duration::from_secs_f32(
                                    self.current_time.load(Ordering::Relaxed) as f32
                                        / 44100.0
                                        / 2.0,
                                ),
                                std::time::Duration::from_secs_f32(self.total_time),
                            )
                        }
                        .height(iced::Length::Units(40))
                        .spacing(20),
                    )
                    .padding(20),
            )
            .into()
    }
}

impl App {
    fn song_loaded(&mut self, song: &sc::Song) -> iced::Command<Message> {
        info!("Song loaded: {}", song.title);
        self.song_cache.write(song.object.clone(), song.clone());

        if let Page::Playlist(playlist_page) = &mut self.page {
            playlist_page.song_loaded(song, self.image_cache.clone());
        }

        Command::none()
    }

    fn queue_song(&mut self, song: &sc::Song) -> iced::Command<Message> {
        for media in song.media.clone().transcodings {
            if media.format.mime_type == "audio/mpeg" {
                let player = self.player.clone();
                let id = song.object.id;
                return async move {
                    tokio::task::spawn(async move {
                        if let Ok(m3u8) = media.resolve().await {
                            player.lock().await.queue(&m3u8, id).await.unwrap();
                        }
                    });
                    Message::None
                }
                .into();
            }
        }

        warn!("No transcoding available for song");

        Command::none()
    }
}

struct QueuedTrackRecipe {
    watch: watch::Receiver<VecDeque<audio::TrackId>>,
}

impl<H, I> iced_native::subscription::Recipe<H, I> for QueuedTrackRecipe
where
    H: std::hash::Hasher,
{
    type Output = Message;

    fn hash(&self, state: &mut H) {
        use std::hash::Hash;

        std::any::TypeId::of::<Self>().hash(state);
        0.hash(state);
    }

    fn stream(self: Box<Self>, _input: BoxStream<I>) -> BoxStream<Self::Output> {
        Box::pin(futures::stream::unfold(self, |mut state| async move {
            state.watch.changed().await.map_or(None, |_| {
                let cloned = state.watch.borrow().clone();
                Some((Message::QueueChanged(cloned), state))
            })
        }))
    }
}
