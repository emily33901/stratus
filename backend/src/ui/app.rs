use audio::HlsPlayer;
use futures::stream::BoxStream;
use iced::{time, Container};

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
    // queue: VecDeque<audio::TrackId>,
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
            // queue: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    None,
    Tick,
    PlaylistClicked(sc::Playlist),
    SongLoaded(sc::Song),
    UserLoaded(sc::User),
    ImageLoaded((String, Handle)),
    SongQueue(sc::Song),
    Resume,
    Pause,
    Skip,
    QueueChanged(VecDeque<audio::SongId>),
    PlaylistFilterChange(String),
    QueuePlaylist,
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
                let playlist = SoundCloud::user(Id::Url("https://soundcloud.com/emilydotgg"))
                    .await
                    .unwrap()
                    .likes()
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
        // match &message {
        //     Message::None | Message::Tick => (),
        //     message => info!("{:?}", message),
        // }

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
            Message::UserLoaded(user) => self.user_loaded(&user),
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
                self.controls.queue = queue;
                async { Message::None }.into()
            }
            Message::PlaylistFilterChange(string) => self.playlist_filter_changed(&string),
            _ => Command::none(),
        };

        use backoff::ExponentialBackoff;

        fn make_backoff() -> ExponentialBackoff {
            ExponentialBackoff {
                initial_interval: std::time::Duration::from_secs_f32(10.0),
                randomization_factor: 0.5,
                ..Default::default()
            }
        }

        // Queue loading images that need it
        let image_loads = Command::batch(self.image_cache.needs_loading().into_iter().map(|url| {
            async {
                info!("Loading image: {}", url);

                let bytes = backoff::future::retry(make_backoff(), || async {
                    let response = reqwest::get(&url).await?;
                    Ok(response.bytes().await.unwrap().to_vec())
                })
                .await
                .unwrap();

                Message::ImageLoaded((url, Handle::from_memory(bytes)))
            }
            .into()
        }));

        let song_loads =
            Command::batch(self.song_cache.needs_loading().into_iter().map(|object| {
                async move {
                    info!("Loading song: {}", object.id);

                    backoff::future::retry(make_backoff(), || async {
                        let song = SoundCloud::song(Id::Id(object.id)).await?;
                        Ok(Message::SongLoaded(song))
                    })
                    .await
                    .unwrap()
                }
                .into()
            }));

        let user_loads =
            Command::batch(self.user_cache.needs_loading().into_iter().map(|object| {
                async move {
                    info!("Loading user: {}", object.id);

                    backoff::future::retry(make_backoff(), || async {
                        let user = SoundCloud::user(Id::Id(object.id)).await?;
                        Ok(Message::UserLoaded(user))
                    })
                    .await
                    .unwrap()
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

        Command::batch([msg_command, image_loads, song_loads, user_loads, update_pos])
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
        Container::new(
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
                ),
        )
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .center_x()
        .center_y()
        .style(crate::ui::style::Theme::Dark)
        .into()
    }
}

impl App {
    fn song_loaded(&mut self, song: &sc::Song) -> iced::Command<Message> {
        info!("Song loaded: {}", song.title);
        self.song_cache.write(song.object.clone(), song.clone());

        if let Page::Playlist(playlist_page) = &mut self.page {
            playlist_page.song_loaded(song, self.image_cache.clone(), self.user_cache.clone())
        } else {
            Command::none()
        }
    }

    fn user_loaded(&mut self, user: &sc::User) -> iced::Command<Message> {
        info!("User loaded: {}", user.username);
        self.user_cache.write(user.object.clone(), user.clone());
        if let Page::Playlist(page) = &mut self.page {
            page.user_loaded(user, self.image_cache.clone())
        } else {
            Command::none()
        }
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

    fn playlist_filter_changed(&mut self, string: &str) -> iced::Command<Message> {
        if let Page::Playlist(page) = &mut self.page {
            page.filter_changed(string);
        }

        Command::none()
    }
}

struct QueuedTrackRecipe {
    watch: watch::Receiver<VecDeque<audio::SongId>>,
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
