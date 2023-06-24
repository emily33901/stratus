use audio::HlsPlayer;
use futures::stream::BoxStream;

use crate::downloader;

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
    navigation: VecDeque<Page>,
    cur_page_index: usize,

    store: Arc<model::Store>,

    player: Arc<audio::HlsPlayer>,
    controls: ControlsElement,
}

impl App {
    pub fn new(store: Arc<model::Store>) -> Self {
        let player = Arc::new(HlsPlayer::new(Arc::new(downloader::Downloader::new(
            store.clone(),
        ))));

        let navigation = vec![Page::default()].into();

        let zelf = Self {
            navigation,
            store,
            player,
            controls: ControlsElement::new(),
            cur_page_index: 0,
        };

        zelf
    }

    fn push_page(&mut self, page: Page) {
        // Check what cur_page is and truncate the navigation vec back to there
        if self.cur_page_index < self.navigation.len() - 1 {
            self.navigation.truncate(self.cur_page_index + 1)
        }

        self.navigation.push_back(page);
        self.cur_page_index += 1;
    }

    fn page_mut(&mut self) -> &mut Page {
        &mut self.navigation[self.cur_page_index]
    }

    fn page(&self) -> &Page {
        &self.navigation[self.cur_page_index]
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
    PageChange(isize),
    PageScroll(f32),
    VolumeChange(f32),
    NavigateForward,
    NavigateBack,
    QueuePlaylist,
    LoopingChanged,
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
                // TODO(emily): This is so stupid. Please either have
                // both as seconds, or both as sample rates
                self.controls.set_player_state(state.clone());

                return Command::none();
            }
            _ => {}
        };

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

                self.push_page(Page::User(UserPage::new(user.clone(), &self.store)));

                let store = self.store.clone();

                // TODO(emily): These Pages should eb components and then they cn make these requests on their own
                // without us having to do this GARBAGE here.
                Command::perform(
                    async move { store.likes(&user.id).await.unwrap() },
                    Message::PlaylistResolved,
                )
            }
            Message::PlaylistClicked(playlist) => {
                self.push_page(Page::Playlist(PlaylistPage::new(playlist)));
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

            Message::PageChange(amount) => {
                match self.page_mut() {
                    Page::Main => todo!(),
                    Page::Playlist(playlist_page) => playlist_page.page_changed(amount),
                    Page::User(_) => todo!(),
                };

                Command::none()
            }
            Message::PageScroll(amount) => {
                match self.page_mut() {
                    Page::Main => todo!(),
                    Page::Playlist(playlist) => playlist.page_scroll(amount),
                    Page::User(user) => user.page_scroll(amount),
                };
                Command::none()
            }
            Message::VolumeChange(volume) => {
                self.controls.volume_changed(volume);
                let player = self.player.clone();
                Command::perform(
                    async move { player.volume(volume).await.unwrap() },
                    Message::None,
                )
            }
            Message::NavigateBack => {
                self.cur_page_index = self.cur_page_index.saturating_sub(1);
                Command::none()
            }
            Message::NavigateForward => {
                self.cur_page_index = self.cur_page_index.saturating_add(1);
                if self.cur_page_index >= self.navigation.len() {
                    self.cur_page_index = self.navigation.len() - 1
                }
                Command::none()
            }
            Message::LoopingChanged => {
                // Get looping from controls
                let looping = self.controls.rotate_looping();
                // Tell player
                let player = self.player.clone();
                Command::perform(
                    async move {
                        player
                            .looping(match looping {
                                0 => audio::Looping::LoopOne,
                                1 => audio::Looping::Loop,
                                2 => audio::Looping::None,
                                _ => unreachable!(),
                            })
                            .await
                            .unwrap();
                    },
                    Message::None,
                )
            }
        }
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
        widget::container(widget::column!(
            widget::row!(
                widget::button(widget::text("<")).on_press(Message::NavigateBack),
                widget::button(widget::text(">")).on_press(Message::NavigateForward)
            ),
            widget::container(match self.page() {
                Page::Main => widget::text("Main page").into(),
                Page::Playlist(playlist_page) => playlist_page.view(),
                Page::User(user_page) => user_page.view(),
            })
            .height(iced::Length::FillPortion(1)),
            widget::container(widget::column!(
                widget::row!().height(iced::Length::Fixed(10.0)),
                self.controls.view()
            )),
        ))
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .padding(10)
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
    fn playlist_loaded(&mut self, playlist: Arc<model::Playlist>) -> Command<Message> {
        info!("Playlist loaded");
        match self.page_mut() {
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
        match self.page_mut() {
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
        if let Page::Playlist(page) = self.page_mut() {
            page.filter_changed(string)
        } else {
            Command::none()
        }
    }

    fn queue_playlist(&mut self) -> iced::Command<Message> {
        if let Page::Playlist(page) = self.page() {
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

impl<T> iced::advanced::subscription::Recipe for WatchRecipe<T>
where
    T: 'static + std::fmt::Debug + Clone + Send + Sync,
{
    type Output = T;

    fn hash(&self, state: &mut iced::advanced::Hasher) {
        use std::hash::Hash;

        self.0.hash(state);
        std::any::TypeId::of::<Self>().hash(state);
    }

    fn stream(
        self: Box<Self>,
        _input: iced::advanced::subscription::EventStream,
    ) -> BoxStream<'static, Self::Output> {
        Box::pin(futures::stream::unfold(self, |mut state| async move {
            // Wait for watcher to change then produce a value
            state.1.changed().await.map_or(None, |_| {
                let value = state.1.borrow().clone();
                Some((value, state))
            })
        }))
    }
}
