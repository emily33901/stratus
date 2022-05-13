use audio::HlsPlayer;
use futures::stream::BoxStream;
use iced::{time, Container};

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

use async_trait::async_trait;

use eyre::{eyre, Result};
use iced::image::Handle;
use iced::pure::{column, container, scrollable, text, Application, Element};
use iced::{self, executor, Command};
use log::{info, warn};

use super::cache::{ImageCache, SongCache, UserCache};
use super::controls::ControlsElement;
use super::playlist_page::PlaylistPage;
use super::user_page::UserPage;
use crate::sc::api::model::Transcoding;
use crate::sc::{self, Id, SoundCloud};

enum Page {
    Main,
    Playlist(PlaylistPage),
    User(UserPage),
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
    controls: ControlsElement,
}

impl Default for App {
    fn default() -> Self {
        let song_cache = Arc::new(SongCache::default());
        let player = Arc::new(Mutex::new(HlsPlayer::new(Arc::new(Downloader::new(
            song_cache.clone(),
        )))));
        let cur_track_rx = player.blocking_lock().cur_song();

        let mut zelf = Self {
            page: Default::default(),
            playlist: Default::default(),
            image_cache: Default::default(),
            song_cache: song_cache.clone(),
            user_cache: Default::default(),
            player,
            current_time: Default::default(),
            total_time: Default::default(),
            controls: ControlsElement::new(song_cache, cur_track_rx),
            // queue: Default::default(),
        };

        zelf
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    None(()),
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

    // UI
    UserClicked(sc::User),
}

impl Message {
    pub(crate) fn none() -> Message {
        Message::None(())
    }
}

struct Downloader {
    client: reqwest::Client,
    song_cache: Arc<SongCache>,
}

impl Downloader {
    fn new(song_cache: Arc<SongCache>) -> Self {
        Self {
            client: reqwest::Client::new(),
            song_cache,
        }
    }
}

#[async_trait]
impl audio::Downloader for Downloader {
    async fn download_chunk(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.client.get(url).send().await?;
        // make sure that if the server returns an error (e.g. Forbidden)
        // that we pass it back up to whoever called us
        Ok(response.error_for_status()?.bytes().await?.to_vec())
    }

    async fn playlist(&self, id: audio::SongId) -> Result<String> {
        // Try and get mpeg transcoding from song
        let (title, transcodings) = self
            .song_cache
            .try_get(&sc::Object {
                id,
                kind: "track".into(),
                ..Default::default()
            })
            .map(|song| (song.title.clone(), song.media.transcodings.clone()))
            .ok_or(eyre!("No such song {} in song cache", id))?;

        if let Some(transcoding) = transcodings
            .iter()
            .find(|t| t.format.mime_type == "audio/mpeg")
        {
            let result = transcoding.resolve().await;
            Ok(result?)
        } else {
            warn!(
                "Song {} missing mpeg transcoding (available transcodings were {:?})",
                &title, &transcodings
            );
            Err(eyre!("No such mpeg transcoding for SongId {}", id))
        }
    }
}

impl Application for App {
    type Executor = executor::Default;

    type Message = Message;

    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (
            Self::default(),
            Command::perform(
                async {
                    let playlist = SoundCloud::user(Id::Url("https://soundcloud.com/emilydotgg"))
                        .await
                        .unwrap()
                        .likes()
                        .await
                        .unwrap();

                    SoundCloud::frame();

                    playlist
                },
                |playlist| Message::PlaylistClicked(playlist),
            ),
        )
    }

    fn title(&self) -> String {
        "stratus".into()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
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
                    Command::perform(
                        async move {
                            song.preload().await;
                        },
                        Message::None,
                    )
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
                Command::perform(
                    async move {
                        let player = player.lock().await;
                        player.resume().await.unwrap();
                    },
                    Message::None,
                )
            }
            Message::Pause => {
                let player = self.player.clone();
                Command::perform(
                    async move {
                        let player = player.lock().await;
                        player.pause().await.unwrap();
                    },
                    Message::None,
                )
            }
            Message::Skip => {
                let player = self.player.clone();
                Command::perform(
                    async move {
                        let player = player.lock().await;
                        player.skip().await.unwrap();
                    },
                    Message::None,
                )
            }
            Message::QueueChanged(queue) => {
                self.controls.queue = queue;
                Command::none()
            }
            Message::QueuePlaylist => self.queue_playlist(),
            Message::PlaylistFilterChange(string) => self.playlist_filter_changed(&string),
            Message::UserClicked(user) => {
                info!("User clicked");

                self.page = Page::User(UserPage::new(user, &self.image_cache));

                Command::none()
            }
            _ => Command::none(),
        };

        use backoff::ExponentialBackoff;

        fn make_backoff() -> ExponentialBackoff {
            ExponentialBackoff {
                initial_interval: std::time::Duration::from_secs_f32(15.0),
                randomization_factor: 0.5,
                ..Default::default()
            }
        }

        // Queue loading images that need it
        let image_loads = Command::batch(self.image_cache.needs_loading().into_iter().map(|url| {
            let url2 = url.clone();
            Command::perform(
                async move {
                    info!("Loading image: {}", url);

                    backoff::future::retry(make_backoff(), || async {
                        let response = reqwest::get(&url).await?;
                        Ok(response.bytes().await.unwrap().to_vec())
                    })
                    .await
                    .unwrap()
                },
                move |bytes| Message::ImageLoaded((url2.clone(), Handle::from_memory(bytes))),
            )
            .into()
        }));

        let song_loads =
            Command::batch(self.song_cache.needs_loading().into_iter().map(|object| {
                Command::perform(
                    async move {
                        info!("Loading song: {}", object.id);

                        backoff::future::retry(make_backoff(), || async {
                            let song = SoundCloud::song(Id::Id(object.id)).await?;
                            Ok(song)
                        })
                        .await
                        .unwrap()
                    },
                    Message::SongLoaded,
                )
            }));

        let user_loads =
            Command::batch(self.user_cache.needs_loading().into_iter().map(|object| {
                Command::perform(
                    async move {
                        info!("Loading user: {}", object.id);

                        backoff::future::retry(make_backoff(), || async {
                            let user = SoundCloud::user(Id::Id(object.id)).await?;
                            Ok(user)
                        })
                        .await
                        .unwrap()
                    },
                    Message::UserLoaded,
                )
            }));

        let current_time = self.current_time.clone();
        let player = self.player.clone();

        let update_pos = Command::perform(
            async move {
                current_time.store(player.lock().await.position(), Ordering::Release);
            },
            Message::None,
        );
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

    fn view(&self) -> Element<Self::Message> {
        container(
            column()
                .push(
                    scrollable(
                        container(match &self.page {
                            Page::Main => text("Main page").into(),
                            Page::Playlist(playlist_page) => playlist_page.view(),
                            Page::User(user_page) => user_page.view(),
                        })
                        .padding(40),
                    )
                    .height(iced::Length::FillPortion(1)),
                )
                .push({
                    container(self.controls.view(
                        std::time::Duration::from_secs_f32(
                            self.current_time.load(Ordering::Relaxed) as f32 / 44100.0 / 2.0,
                        ),
                        std::time::Duration::from_secs_f32(self.total_time),
                    ))
                    .height(iced::Length::Units(80))
                }),
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

    fn queue_song(&self, song: &sc::Song) -> iced::Command<Message> {
        for media in song.media.clone().transcodings {
            if media.format.mime_type == "audio/mpeg" {
                let player = self.player.clone();
                let id = song.object.id;
                return Command::perform(
                    async move {
                        tokio::task::spawn(async move {
                            if let Ok(m3u8) = media.resolve().await {
                                player.lock().await.queue(&m3u8, id).await.unwrap();
                            }
                        });
                    },
                    Message::None,
                );
            }
        }

        warn!("No transcoding available for song {}?", &song.title);

        Command::none()
    }

    fn playlist_filter_changed(&mut self, string: &str) -> iced::Command<Message> {
        if let Page::Playlist(page) = &mut self.page {
            page.filter_changed(string);
        }

        Command::none()
    }

    fn queue_playlist(&mut self) -> iced::Command<Message> {
        if let Page::Playlist(page) = &self.page {
            iced::Command::batch(page.songs().map(|song| self.queue_song(song)))
        } else {
            Command::none()
        }
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
