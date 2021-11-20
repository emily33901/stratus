pub use api::model::{Media, Object, Playlist, Song, User};
use eyre::Result;

pub mod api {
    use image::ImageFormat;
    use log::{info, warn};

    pub mod model {

        use serde::{Deserialize, Deserializer, Serialize};
        use static_assertions::assert_impl_all;

        #[derive(Debug, Deserialize, Serialize, Clone)]
        pub struct ObjectInside {
            pub id: u64,
            pub kind: String,
            #[serde(rename = "permalink_url")]
            pub url: Option<String>,
            pub uri: Option<String>,
        }

        #[derive(Debug, Serialize, Clone, Default)]
        pub struct Object {
            pub id: u64,
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

        impl Eq for Object {}

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

        #[derive(Deserialize, Serialize, Debug)]
        pub struct User {
            #[serde(flatten)]
            pub object: Object,

            pub username: String,
            #[serde(rename = "avatar_url")]
            pub avatar: Option<String>,
        }

        #[derive(Deserialize, Serialize, Debug, Default, Clone)]
        pub struct Song {
            #[serde(flatten)]
            pub object: Object,

            pub user: Object,
            #[serde(rename = "artwork_url")]
            pub artwork: Option<String>,
            pub title: String,
            pub media: Media,
            // This is in milliseconds
            pub full_duration: usize,
        }

        #[derive(Deserialize, Serialize, Debug, Clone, Default)]
        pub struct Playlist {
            #[serde(flatten)]
            pub object: Object,

            #[serde(rename = "artwork_url")]
            pub artwork: Option<String>,
            pub user: Object,
            #[serde(rename = "tracks")]
            pub songs: Vec<Object>,
            pub title: String,
        }

        #[derive(Deserialize, Serialize, Debug, Clone, Default)]
        pub struct HlsPlaylist {
            pub url: String,
        }
    }

    impl Object {
        pub async fn preload(&self) {
            // TEMP(emily):
            // do not preload since we dont cache right now
            info!("Not preloading");
            return;

            info!("Preloading: {}({})", self.kind, self.id);
            match self.kind.as_str() {
                "track" => {
                    let _ = model::Song::resolve(self.id).await;
                }
                "user" => {
                    let _ = model::User::resolve(self.id).await;
                }
                "playlist" => {
                    let _ = model::Playlist::resolve(self.id).await;
                }
                _ => {
                    warn!("Not preloading unknown type: {}", self.kind);
                }
            };
        }
    }

    use std::io::Cursor;

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
        static ref COMMON_PARAMS: [(&'static str, &'static str); 1] = [("client_id", CLIENT_ID)];
    }

    async fn image(url: &str) -> Result<image::DynamicImage> {
        let client = reqwest::Client::new();

        let headers = COMMON_HEADERS.clone();
        let params = *COMMON_PARAMS;

        let response = client
            .get(url)
            .query(&params)
            .headers(headers)
            .send()
            .await?;

        let bytes = response.bytes().await?;

        Ok(image::io::Reader::with_format(Cursor::new(bytes), ImageFormat::Jpeg).decode()?)
    }

    pub async fn hls_playlist(url: &str) -> Result<String> {
        let client = reqwest::Client::new();

        let headers = COMMON_HEADERS.clone();
        let params = *COMMON_PARAMS;

        let response = client
            .get(url)
            .query(&params)
            .headers(headers)
            .send()
            .await?;

        let playlist: model::HlsPlaylist = response.json().await?;

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

    async fn object<T: for<'de> serde::Deserialize<'de>>(endpoint: &str, id: u64) -> Result<T> {
        info!("Cache miss for {}", id);

        let client = reqwest::Client::new();

        let headers = COMMON_HEADERS.clone();
        let params = *COMMON_PARAMS;

        let final_endpoint = format!("{}/{}/{}", API_ORIGIN, endpoint, id);

        info!("GETting {}", final_endpoint);

        let response = client
            .get(final_endpoint)
            .query(&params)
            .headers(headers)
            .send()
            .await?;

        let text = response.text().await?;
        let object = serde_json::from_str(&text)
            .map_err(|e| eyre!(e).wrap_err(eyre!("T was {}", std::any::type_name::<T>())))?;

        Ok(object)
    }

    impl model::User {
        pub async fn resolve(id: u64) -> Result<Self> {
            object("users", id).await
        }
    }

    impl model::Song {
        pub async fn resolve(id: u64) -> Result<Self> {
            object("tracks", id).await
        }
    }

    impl model::Playlist {
        pub async fn resolve(id: u64) -> Result<Self> {
            object("playlists", id).await
        }
    }

    impl model::Transcoding {
        pub async fn resolve(&self) -> Result<String> {
            Ok(hls_playlist(&self.url).await?)
        }
    }

    pub async fn resolve_url(url: &str) -> Result<Object> {
        let client = reqwest::Client::new();

        let headers = COMMON_HEADERS.clone();
        let mut params = COMMON_PARAMS.to_vec();
        params.push(("url", url));

        let response = client
            .get(format!("{}/{}", API_ORIGIN, "resolve"))
            .query(&params)
            .headers(headers)
            .send()
            .await?;

        let text = response.text().await?;
        let object = serde_json::from_str(&text)?;

        Ok(object)
    }
}

pub enum Id<'a> {
    Url(&'a str),
    Id(u64),
}

pub struct SoundCloud {}

impl SoundCloud {
    pub async fn song(id: Id<'_>) -> Result<Song> {
        let id = match id {
            Id::Url(url) => api::resolve_url(url).await?.id,
            Id::Id(id) => id,
        };

        Ok(Song::resolve(id).await?)
    }

    pub async fn user(id: Id<'_>) -> Result<User> {
        let id = match id {
            Id::Url(url) => api::resolve_url(url).await?.id,
            Id::Id(id) => id,
        };

        Ok(User::resolve(id).await?)
    }

    pub async fn playlist(id: Id<'_>) -> Result<Playlist> {
        let id = match id {
            Id::Url(url) => api::resolve_url(url).await?.id,
            Id::Id(id) => id,
        };
        Ok(Playlist::resolve(id).await?)
    }

    pub fn frame() {}
}
