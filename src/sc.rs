pub use api::model::{Media, Object, Playlist, Song, User};
use eyre::Result;

pub mod api {
    use log::info;

    use serde::Deserialize;

    use super::Id;

    pub mod model {
        use serde::{Deserialize, Deserializer, Serialize};
        use static_assertions::assert_impl_all;

        /// Type that represents objects
        /// 'Real' (i.e. actual things from the API) have positive ids
        /// 'Fake' (i.e. things that we generate and pretend are real) have
        /// negative ids
        pub type Id = i64;

        #[derive(Debug, Deserialize, Serialize, Clone)]
        pub struct ObjectInside {
            pub id: Id,
            pub kind: String,
            #[serde(rename = "permalink_url")]
            pub url: Option<String>,
            pub uri: Option<String>,
        }

        #[derive(Debug, Serialize, Clone, Default, Eq)]
        pub struct Object {
            pub id: Id,
            pub kind: String,
            #[serde(rename = "permalink_url")]
            pub url: Option<String>,
            pub uri: Option<String>,
        }

        impl PartialEq for Object {
            fn eq(&self, other: &Self) -> bool {
                self.id == other.id
            }
        }

        impl std::hash::Hash for Object {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.id.hash(state);
            }
        }

        assert_impl_all!(Object: Send, Sync);

        impl<'de> Deserialize<'de> for Object {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let ObjectInside { id, kind, url, uri } = ObjectInside::deserialize(deserializer)?;
                Ok(Self { id, kind, url, uri })
            }
        }

        pub trait Objectable {
            fn object(&self) -> &Object;
        }

        #[derive(Deserialize, Serialize, Debug, Default, Clone)]
        pub struct Format {
            pub mime_type: String,
            pub protocol: String,
        }

        assert_impl_all!(Format: Send, Sync);

        #[derive(Deserialize, Serialize, Debug, Default, Clone)]
        pub struct Transcoding {
            pub url: String,
            pub format: Format,
        }

        assert_impl_all!(Transcoding: Send, Sync);

        #[derive(Deserialize, Serialize, Debug, Default, Clone)]
        pub struct Media {
            pub transcodings: Vec<Transcoding>,
        }

        #[derive(Deserialize, Serialize, Debug, Default, Clone)]
        pub struct User {
            #[serde(flatten)]
            pub object: Object,

            pub username: String,
            #[serde(rename = "avatar_url")]
            pub avatar: Option<String>,
        }

        impl Objectable for User {
            fn object(&self) -> &Object {
                &self.object
            }
        }

        #[derive(Deserialize, Serialize, Debug, Default, Clone)]
        pub struct Song {
            #[serde(flatten)]
            pub object: Object,

            pub user: User,
            #[serde(rename = "artwork_url")]
            pub artwork: Option<String>,
            pub title: String,
            pub media: Media,
            // This is in milliseconds
            pub full_duration: usize,
        }

        #[derive(Deserialize, Serialize, Debug, Default, Clone)]
        pub struct BlackboxSong {
            pub id: i64,
        }

        impl Objectable for Song {
            fn object(&self) -> &Object {
                return &self.object;
            }
        }

        #[derive(Deserialize, Serialize, Debug, Clone, Default)]
        pub struct Playlist {
            #[serde(flatten)]
            pub object: Object,

            #[serde(rename = "artwork_url")]
            pub artwork: Option<String>,
            pub user: Object,
            #[serde(rename = "tracks")]
            pub songs: Vec<serde_json::Value>,
            pub title: String,
        }

        impl Objectable for Playlist {
            fn object(&self) -> &Object {
                return &self.object;
            }
        }

        #[derive(Deserialize, Serialize, Debug, Clone, Default)]
        pub struct HlsPlaylist {
            pub url: String,
        }
    }

    use std::sync::atomic::AtomicI64;

    use eyre::{eyre, Result, WrapErr};
    use lazy_static::lazy_static;
    use reqwest::header;

    use self::model::Object;

    const API_ORIGIN: &str = "https://api-widget.soundcloud.com";
    const CLIENT_ID: &str = env!("STRATUS_CLIENT_ID");
    const USER_AGENT: &str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:88.0) Gecko/20100101 Firefox/88.0";

    lazy_static! {
        static ref COMMON_HEADERS: header::HeaderMap = {
            let mut headers = header::HeaderMap::new();
            headers.insert(header::HOST, "api-widget.soundcloud.com".parse().unwrap());
            headers.insert(header::ORIGIN, "w.soundcloud.com".parse().unwrap());
            headers.insert(header::USER_AGENT, USER_AGENT.parse().unwrap());

            headers
        };
        static ref COMMON_PARAMS: [(String, String); 1] = [("client_id".into(), CLIENT_ID.into())];
    }

    pub async fn image(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
        // let headers = COMMON_HEADERS.clone();
        // let params = COMMON_PARAMS.clone();

        let response = client
            .get(url)
            // .query(&params)
            // .headers(headers)
            .send()
            .await?;

        Ok(response.bytes().await?.to_vec())
    }

    pub async fn hls_playlist(url: &str) -> Result<String> {
        let client = reqwest::Client::new();

        let headers = COMMON_HEADERS.clone();
        let params = COMMON_PARAMS.clone();

        let response = client
            .get(url)
            .query(&params)
            .headers(headers)
            .send()
            .await?;

        let text = response.text().await?;

        let playlist: model::HlsPlaylist = serde_json::from_str(&text).wrap_err(format!(
            "Failed to decode HLS Playlist from API (text was {})",
            &text
        ))?;

        // now get the actual m3u8 from the response object
        let mut headers = header::HeaderMap::new();
        headers.insert(header::HOST, "cf-hls-media.sndcdn.com".parse()?);
        headers.insert(
            header::ACCEPT,
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"
                .parse()?,
        );

        let response = client.get(playlist.url).headers(headers).send().await?;
        let playlist = response.text().await?;

        Ok(playlist)
    }

    #[derive(Debug, Default)]
    pub struct Endpoint {
        endpoint: String,
        params: Option<Vec<(String, String)>>,
    }

    impl Endpoint {
        pub fn from_id<F: Fn(i64) -> String>(
            id: super::Id<'_>,
            format_endpoint: F,
            params: Option<Vec<(String, String)>>,
        ) -> Endpoint {
            match id {
                super::Id::Url(url) => Endpoint {
                    endpoint: "resolve".into(),
                    params: Some({
                        let mut v = vec![("url".into(), url.into())];
                        v.append(&mut params.unwrap_or_default());
                        v
                    }),
                },
                super::Id::Id(id) => Endpoint {
                    endpoint: format_endpoint(id),
                    params,
                },
            }
        }
    }

    pub async fn object<T: for<'de> serde::Deserialize<'de>>(
        client: &reqwest::Client,
        mut endpoint: Endpoint,
    ) -> Result<T> {
        let headers = COMMON_HEADERS.clone();
        let mut params: Vec<_> = COMMON_PARAMS.clone().into();
        if let Some(extra_params) = endpoint.params.take() {
            params.extend(extra_params);
        }

        let final_endpoint = format!("{}/{}", API_ORIGIN, endpoint.endpoint);

        info!("GETting {}", final_endpoint);

        let response = client
            .get(final_endpoint)
            .query(&params)
            .headers(headers)
            .send()
            .await?;

        let text = response.text().await?;
        // let v: serde_json::Value = serde_json::from_str(&text)?;
        // let text = serde_json::to_string_pretty(&v)?;

        let object = serde_json::from_str(&text)
            .map_err(|e| eyre!(e).wrap_err(eyre!("T was {}", std::any::type_name::<T>(),)))?;

        Ok(object)
    }

    fn next_fake_id() -> i64 {
        static NEXT_FAKE_ID: AtomicI64 = AtomicI64::new(-1);
        NEXT_FAKE_ID.fetch_sub(1, std::sync::atomic::Ordering::SeqCst)
    }

    impl model::User {
        pub async fn resolve(client: &reqwest::Client, id: Id<'_>) -> Result<Self> {
            object(
                client,
                Endpoint::from_id(id, |id| format!("users/{}", id), None),
            )
            .await
        }

        pub async fn likes(&self, client: &reqwest::Client) -> Result<model::Playlist> {
            #[derive(Deserialize)]
            struct Like {
                track: serde_json::Value,
            }

            #[derive(Deserialize)]
            struct Likes {
                collection: Vec<Like>,
            }

            info!("Loading likes");

            let endpoint = Endpoint {
                endpoint: format!("users/{}/track_likes", self.object.id),
                params: Some(vec![("limit".into(), "8000".into())]),
            };

            let likes: Likes = object(client, endpoint).await?;
            let id = next_fake_id();

            Ok(model::Playlist {
                object: Object {
                    id,
                    kind: "likes".into(),
                    uri: None,
                    url: None,
                },
                artwork: self.avatar.clone(),
                user: self.object.clone(),
                songs: likes.collection.into_iter().map(|x| x.track).collect(),
                title: format!("Liked by {}", self.username),
            })
        }

        pub async fn songs(&self, client: &reqwest::Client) -> Result<Option<model::Playlist>> {
            let endpoint = Endpoint {
                endpoint: format!("users/{}/tracks", self.object.id),
                params: Some(vec![("limit".into(), "50".into())]),
                ..Default::default()
            };

            #[derive(Deserialize)]
            struct Songs {
                collection: Vec<serde_json::Value>,
            }

            let songs: Option<Songs> = object(client, endpoint).await?;
            let id = next_fake_id();
            Ok(songs.map(|songs| model::Playlist {
                object: Object {
                    id,
                    kind: "songs".into(),
                    uri: None,
                    url: None,
                },
                artwork: self.avatar.clone(),
                user: self.object.clone(),
                songs: songs.collection.into_iter().map(|x| x).collect(),
                title: format!("Tracks by {}", self.username),
            }))
        }
    }

    impl model::Song {
        pub async fn resolve(client: &reqwest::Client, id: Id<'_>) -> Result<Self> {
            object(
                client,
                Endpoint::from_id(id, |id| format!("tracks/{}", id), None),
            )
            .await
        }
    }

    impl model::Playlist {
        pub async fn resolve(client: &reqwest::Client, id: Id<'_>) -> Result<Self> {
            object(
                client,
                Endpoint::from_id(id, |id| format!("playlists/{}", id), None),
            )
            .await
        }
    }

    impl model::Transcoding {
        pub async fn resolve(&self) -> Result<String> {
            Ok(hls_playlist(&self.url).await?)
        }
    }
}

pub enum Id<'a> {
    Url(&'a str),
    Id(i64),
}

#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum OwnedId {
    Url(String),
    Id(i64),
}

impl<'a> From<Id<'a>> for OwnedId {
    fn from(id: Id<'a>) -> Self {
        match id {
            Id::Url(url) => Self::Url(url.into()),
            Id::Id(id) => Self::Id(id),
        }
    }
}

impl<'a> From<&'a OwnedId> for Id<'a> {
    fn from(owned: &'a OwnedId) -> Self {
        match owned {
            OwnedId::Url(url) => Self::Url(&url),
            OwnedId::Id(id) => Self::Id(*id),
        }
    }
}

pub struct SoundCloud {
    client: reqwest::Client,
}

impl SoundCloud {
    pub fn new() -> Self {
        Self {
            client: Default::default(),
        }
    }

    pub async fn song(&self, id: Id<'_>) -> Result<Song> {
        Ok(Song::resolve(&self.client, id).await?)
    }

    pub async fn user(&self, id: Id<'_>) -> Result<User> {
        Ok(User::resolve(&self.client, id).await?)
    }

    pub async fn playlist(&self, id: Id<'_>) -> Result<Playlist> {
        Ok(Playlist::resolve(&self.client, id).await?)
    }

    pub async fn image(&self, url: &str) -> Result<iced::widget::image::Handle> {
        let image = api::image(&self.client, &url.replace("-large", "-t120x120")).await?;
        Ok(iced::widget::image::Handle::from_memory(image))
    }

    pub async fn likes(&self, id: Id<'_>) -> Result<Playlist> {
        let user = self.user(id).await?;
        Ok(user.likes(&self.client).await?)
    }

    pub async fn url(&self, url: &str) -> Result<Object> {
        api::object(
            &self.client,
            api::Endpoint::from_id(Id::Url(url), |x| format!("{x}"), None),
        )
        .await
    }

    pub fn frame() {}
}
