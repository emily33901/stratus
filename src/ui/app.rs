use audio::HlsPlayer;
use futures::stream::BoxStream;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::watch;

use iced::widget;
use iced::{self, executor, Command};
use iced::{Application, Element};
use log::{info, warn};

use super::controls::ControlsElement;
use super::playlist_page::PlaylistPage;
use super::song_list::Display;
use super::user_page::UserPage;
use crate::model::{self, Store};

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

    store: Arc<model::Store>,

    player: Arc<audio::HlsPlayer>,
    player_time: f32,
    total_time: f32,
    // queue: VecDeque<audio::TrackId>,
    controls: ControlsElement,
}

impl App {
    pub fn new(store: Arc<model::Store>) -> Self {
        let player = Arc::new(HlsPlayer::new(Arc::new(downloader::Downloader::new(
            store.clone(),
        ))));

        let zelf = Self {
            page: Default::default(),
            store,
            player,
            player_time: Default::default(),
            total_time: Default::default(),
            controls: ControlsElement::new(),
            // queue: Default::default(),
        };

        zelf
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    None(()),
    Tick,
    PlayerState(audio::PlayerState),

    QueueChanged(VecDeque<audio::SongId>),
    QueueResolved(Vec<Arc<model::Song>>),
    SongListFilterComputed(HashMap<model::Id, Display>),
    PlaylistResolved(Arc<model::Playlist>),
    CurSongChange(Option<audio::SongId>),
    CurSongResolved(Option<Arc<model::Song>>),

    // UI
    UserClicked(Arc<model::User>),
    PlaylistClicked(Arc<model::Playlist>),
    SongQueue(Arc<model::Song>),
    PlaylistFilterChange(String),
    QueuePlaylist,
    Resume,
    Pause,
    Skip,
}

impl Message {
    pub(crate) fn none() -> Message {
        Message::None(())
    }
}

impl Application for App {
    type Executor = executor::Default;

    type Message = Message;

    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        let store = Arc::new(model::Store::new());

        (
            Self::new(store.clone()),
            Command::perform(
                async move {
                    let user_id = store
                        .resolve_url("https://soundcloud.com/emilydotgg")
                        .await
                        .unwrap();
                    let likes = store.likes(&user_id).await.unwrap();
                    // let playlist = store.playlist(&236653468).await.unwrap();
                    likes
                },
                Message::PlaylistClicked,
            ),
        )
    }

    fn title(&self) -> String {
        "stratus".into()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match &message {
            Self::Message::None(_) => return Command::none(),
            Message::PlayerState(state) => {
                self.player_time = state.pos as f32 / state.sample_rate as f32 / 2.0;
                self.total_time = state.total;
                return Command::none();
            }
            _ => {}
        };

        self.handle_message(message)
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        iced::Subscription::batch([
            watch_subscription("player state", self.player.state_rx()).map(Message::PlayerState),
            watch_subscription("player song", self.player.cur_song()).map(Message::CurSongChange),
            watch_subscription("queue changed", self.player.queued_watch())
                .map(Message::QueueChanged),
        ])
    }

    fn view(&self) -> Element<Self::Message> {
        widget::container(
            widget::column!()
                .push(
                    widget::scrollable(
                        widget::container(match &self.page {
                            Page::Main => widget::text("Main page").into(),
                            Page::Playlist(playlist_page) => playlist_page.view(),
                            Page::User(user_page) => user_page.view(),
                        })
                        .padding(40),
                    )
                    .height(iced::Length::FillPortion(1)),
                )
                .push({
                    widget::container(self.controls.view(
                        std::time::Duration::from_secs_f32(self.player_time),
                        std::time::Duration::from_secs_f32(self.total_time),
                    ))
                    .height(iced::Length::Fixed(80.0))
                    .padding(iced::Padding::new(20.0))
                }),
        )
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .center_x()
        .center_y()
        .into()
    }
    fn theme(&self) -> Self::Theme {
        iced::Theme::Dark
    }

    type Theme = iced::Theme;
}

impl App {
    fn handle_message(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::None(_) | Message::Tick | Message::PlayerState(_) => Command::none(),
            Message::PlaylistResolved(playlist) => self.playlist_loaded(playlist),
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
                let store = self.store.clone();
                Command::perform(
                    async move {
                        let tasks = queue.into_iter().map(|id| {
                            tokio::spawn(
                                (|store: Arc<Store>| async move { store.song(&id).await })(
                                    store.clone(),
                                ),
                            )
                        });
                        futures::future::join_all(tasks)
                            .await
                            .iter()
                            .filter_map(|x| x.as_ref().ok())
                            .filter_map(|x| x.as_ref().ok())
                            .cloned()
                            .collect()
                    },
                    Message::QueueResolved,
                )
            }
            Message::QueueResolved(queue) => self.controls.queue_changed(&queue),
            Message::QueuePlaylist => self.queue_playlist(),
            Message::PlaylistFilterChange(string) => self.playlist_filter_changed(&string),
            Message::UserClicked(user) => {
                info!("User clicked");

                self.page = Page::User(UserPage::new(user.clone(), &self.store));

                // Command::perform(
                //     async move { user.songs().await.unwrap() },
                //     Message::PlaylistLoaded,
                // )

                Command::none()
            }
            Message::PlaylistClicked(playlist) => {
                self.page = Page::Playlist(PlaylistPage::new(playlist));
                Command::none()
            }
            Message::SongListFilterComputed(computed) => self.song_list_filter_computed(&computed),
            Message::CurSongChange(Some(id)) => {
                let store = self.store.clone();
                Command::perform(
                    async move { store.song(&id).await.ok() },
                    Message::CurSongResolved,
                )
            }
            Message::CurSongChange(None) => {
                Command::perform(async { None }, Message::CurSongResolved)
            }
            Message::CurSongResolved(song) => {
                self.controls.set_cur_song(song);
                Command::none()
            }
        }
    }

    fn playlist_loaded(&mut self, playlist: Arc<model::Playlist>) -> Command<Message> {
        info!("Playlist loaded");
        match &mut self.page {
            Page::Main => todo!(),
            Page::Playlist(_) => todo!(),
            Page::User(page) => page.update_songs(playlist.clone()),
        };

        Command::none()
    }

    fn song_list_filter_computed(
        &mut self,
        computed: &HashMap<model::Id, Display>,
    ) -> Command<Message> {
        info!("Filter computed");
        match &mut self.page {
            Page::Playlist(page) => page.song_list.filter_computed(computed),
            Page::Main => todo!(),
            Page::User(_) => todo!(),
        }
    }

    fn queue_song(&self, song: &Arc<model::Song>) -> iced::Command<Message> {
        for media in song.media.clone().transcodings {
            if media.format.mime_type == "audio/mpeg" {
                let player = self.player.clone();
                let id = song.id;
                return Command::perform(
                    async move {
                        player.queue(id).await.unwrap();
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
            page.filter_changed(string)
        } else {
            Command::none()
        }
    }

    fn queue_playlist(&mut self) -> iced::Command<Message> {
        if let Page::Playlist(page) = &self.page {
            let player = self.player.clone();
            let ids = page.songs().map(|s| s.id).collect();
            iced::Command::perform(
                async move {
                    player.queue_many(ids).await.unwrap();
                },
                Message::None,
            )
        } else {
            Command::none()
        }
    }
}

fn watch_subscription<T: 'static + std::fmt::Debug + Clone + Send + Sync>(
    id: &str,
    rx: watch::Receiver<T>,
) -> iced::Subscription<T> {
    iced::Subscription::from_recipe(WatchRecipe(id.into(), rx))
}

#[derive(Clone)]
struct WatchRecipe<T>(String, watch::Receiver<T>);

impl<T, H, Event> iced::subscription::Recipe<H, Event> for WatchRecipe<T>
where
    H: std::hash::Hasher,
    T: 'static + std::fmt::Debug + Clone + Send + Sync,
{
    type Output = T;

    fn hash(&self, state: &mut H) {
        use std::hash::Hash;

        self.0.hash(state);
        std::any::TypeId::of::<Self>().hash(state);
    }

    fn stream(self: Box<Self>, _input: BoxStream<Event>) -> BoxStream<Self::Output> {
        Box::pin(futures::stream::unfold(self, |mut state| async move {
            // Wait for watcher to change then produce a value
            state.1.changed().await.map_or(None, |_| {
                let value = state.1.borrow().clone();
                Some((value, state))
            })
        }))
    }
}
