use audio::HlsPlayer;
use futures::stream::BoxStream;
use iced::time;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

use iced::image::Handle;
use iced::pure::{column, container, scrollable, text, Application, Element};
use iced::{self, executor, Command};
use log::{info, warn};

use super::cache::{ImageCache, SongCache, UserCache};
use super::controls::ControlsElement;
use super::playlist_page::PlaylistPage;
use super::user_page::UserPage;

use crate::sc::{self, Id, SoundCloud};

mod downloader;
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

    player: Arc<audio::HlsPlayer>,
    player_time: f32,
    total_time: f32,
    // queue: VecDeque<audio::TrackId>,
    controls: ControlsElement,
}

impl Default for App {
    fn default() -> Self {
        let song_cache = Arc::new(SongCache::default());
        let player = Arc::new(HlsPlayer::new(Arc::new(downloader::Downloader::new(
            song_cache.clone(),
        ))));
        let cur_track_rx = player.cur_song();

        let zelf = Self {
            page: Default::default(),
            playlist: Default::default(),
            image_cache: Default::default(),
            song_cache: song_cache.clone(),
            user_cache: Default::default(),
            player,
            player_time: Default::default(),
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
    CacheLoads,
    PlayerTime(usize),

    SongLoaded(sc::Song),
    UserLoaded(sc::User),
    PlaylistLoaded(sc::Playlist),
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
    PlaylistClicked(sc::Playlist),
}

impl Message {
    pub(crate) fn none() -> Message {
        Message::None(())
    }
}

impl App {
    fn handle_message(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::None(_) | Message::Tick | Message::CacheLoads | Message::PlayerTime(_) => {
                Command::none()
            }
            Message::ImageLoaded((url, handle)) => {
                info!("Image loaded: {}", url);
                self.image_cache.write(url, handle);
                Command::none()
            }
            Message::SongLoaded(song) => self.song_loaded(&song),
            Message::UserLoaded(user) => self.user_loaded(&user),
            Message::PlaylistLoaded(playlist) => self.playlist_loaded(playlist),
            Message::SongQueue(song) => self.queue_song(&song),
            Message::Resume => {
                let player = self.player.clone();
                Command::perform(
                    async move {
                        player.resume().await.unwrap();
                    },
                    Message::None,
                )
            }
            Message::Pause => {
                let player = self.player.clone();
                Command::perform(
                    async move {
                        player.pause().await.unwrap();
                    },
                    Message::None,
                )
            }
            Message::Skip => {
                let player = self.player.clone();
                Command::perform(
                    async move {
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

                self.page = Page::User(UserPage::new(user.clone(), &self.image_cache));

                Command::perform(
                    async move { user.songs().await.unwrap() },
                    Message::PlaylistLoaded,
                )
            }
            Message::PlaylistClicked(playlist) => {
                let playlist2 = playlist.clone();

                self.page = Page::Playlist(PlaylistPage::new(playlist.clone()));

                Command::batch(playlist2.songs.into_iter().map(|song| {
                    let song_cache = self.song_cache.clone();
                    Command::perform(
                        async move {
                            song_cache.try_get(&song);
                        },
                        Message::None,
                    )
                }))
            }
        }
    }

    fn playlist_loaded(&mut self, playlist: sc::Playlist) -> Command<Message> {
        info!("Playlist loaded");
        match &mut self.page {
            Page::Main => todo!(),
            Page::Playlist(_) => todo!(),
            Page::User(page) => page.update_songs(playlist.clone()),
        };
        Command::batch(playlist.songs.into_iter().map(|song| {
            let song_cache = self.song_cache.clone();
            Command::perform(
                async move {
                    song_cache.try_get(&song);
                },
                Message::None,
            )
        }))
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
        match &message {
            Self::Message::None(_) => return Command::none(),
            Self::Message::CacheLoads => {
                use backoff::ExponentialBackoff;
                fn make_backoff() -> ExponentialBackoff {
                    ExponentialBackoff {
                        initial_interval: std::time::Duration::from_secs_f32(15.0),
                        randomization_factor: 0.5,
                        ..Default::default()
                    }
                }

                // Queue loading images that need it
                let image_loads =
                    Command::batch(self.image_cache.needs_loading().into_iter().map(|url| {
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
                            move |bytes| {
                                Message::ImageLoaded((url2.clone(), Handle::from_memory(bytes)))
                            },
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

                return Command::batch([image_loads, song_loads, user_loads]);
            }
            Message::PlayerTime(time) => {
                self.player_time = *time as f32 / 44100.0 / 2.0;
                return Command::none();
            }
            _ => {}
        };

        self.handle_message(message)
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        iced::Subscription::batch([
            time::every(std::time::Duration::from_millis(1000)).map(|_| Message::CacheLoads),
            iced::Subscription::from_recipe(WatchRecipe(self.player.pos_rx(), Message::PlayerTime)),
            iced::Subscription::from_recipe(QueuedTrackRecipe {
                watch: self.player.queued_watch(),
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
                        std::time::Duration::from_secs_f32(self.player_time),
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

        match &mut self.page {
            Page::Main => Command::none(),
            Page::Playlist(playlist_page) => {
                playlist_page
                    .song_list
                    .song_loaded(song, &self.image_cache, &self.user_cache)
            }
            Page::User(user_page) => user_page
                .song_list
                .as_mut()
                .map(|list| list.song_loaded(song, &self.image_cache, &self.user_cache))
                .unwrap_or(Command::none()),
        }
    }

    fn user_loaded(&mut self, user: &sc::User) -> iced::Command<Message> {
        info!("User loaded: {}", user.username);
        self.user_cache.write(user.object.clone(), user.clone());
        match &mut self.page {
            Page::Playlist(page) => page.song_list.user_loaded(user, &self.image_cache),
            Page::User(page) => page
                .song_list
                .as_mut()
                .map(|list| list.user_loaded(user, &self.image_cache))
                .unwrap_or(Command::none()),
            _ => Command::none(),
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
                                player.queue(&m3u8, id).await.unwrap();
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

struct WatchRecipe<T>(watch::Receiver<T>, fn(T) -> Message);

impl<T, H, I> iced_native::subscription::Recipe<H, I> for WatchRecipe<T>
where
    H: std::hash::Hasher,
    T: 'static + Clone + Send + Sync,
{
    type Output = Message;

    fn hash(&self, state: &mut H) {
        use std::hash::Hash;

        std::any::TypeId::of::<Self>().hash(state);
        0.hash(state);
    }

    fn stream(self: Box<Self>, _input: BoxStream<I>) -> BoxStream<Self::Output> {
        Box::pin(futures::stream::unfold(self, |mut state| async move {
            state.0.changed().await.map_or(None, |_| {
                let cloned = state.0.borrow().clone();
                Some((state.1(cloned), state))
            })
        }))
    }
}
