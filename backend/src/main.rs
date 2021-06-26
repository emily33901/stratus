use std::error::Error;

use crate::sc::SoundCloud;

use soundcloud::{App, Client};

mod sc {
    pub mod api {
        use reqwest::{get, header, Request};
        use serde::{Deserialize, Serialize};

        use std::{collections::HashMap, error::Error};

        #[derive(Deserialize, Serialize, Debug)]
        pub struct Format {
            mime_type: String,
            protocol: String,
        }

        #[derive(Deserialize, Serialize, Debug)]
        pub struct Transcoding {
            url: String,
            format: Format,
        }

        #[derive(Deserialize, Serialize, Debug)]
        pub struct Media {
            transcodings: Vec<Transcoding>,
        }

        #[derive(Deserialize, Serialize, Debug)]
        pub struct User {
            pub id: u64,
            #[serde(rename = "permalink_url")]
            pub url: String,
            pub username: String,
            #[serde(rename = "avatar_url")]
            pub avatar: String,
        }

        #[derive(Deserialize, Serialize, Debug)]
        pub struct Song {
            pub id: u64,
            pub user: User,
            #[serde(rename = "artwork_url")]
            pub artwork: String,
            #[serde(rename = "permalink_url")]
            pub url: String,
            pub title: String,
            pub media: Media,
        }

        const API_ENDPOINT: &str = "https://api-widget.soundcloud.com/resolve";
        const CLIENT_ID: &str = "LBCcHmRB8XSStWL6wKH2HPACspQlXg2P";
        const USER_AGENT: &str =
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:88.0) Gecko/20100101 Firefox/88.0";

        pub async fn song(url: &str) -> Result<Song, Box<dyn Error>> {
            let client = reqwest::Client::new();

            let mut headers = header::HeaderMap::new();

            headers.insert(header::HOST, "api-widget.soundcloud.com".parse().unwrap());
            headers.insert(header::ORIGIN, "w.soundcloud.com".parse().unwrap());
            headers.insert(header::USER_AGENT, USER_AGENT.parse().unwrap());

            let params = [("url", url), ("client_id", CLIENT_ID)];

            let response = client
                .get(API_ENDPOINT)
                .query(&params)
                .headers(headers)
                .send()
                .await?;

            let song = response.json().await?;

            return Ok(song);
        }
    }

    use std::{collections::HashMap, error::Error};

    use api::{Media, User};

    #[derive(Debug)]
    pub struct Song {
        id: u64,
        user: u64,
        artwork_url: String,
        url: String,
        title: String,
        media: Media,
    }

    impl Song {
        pub fn from(json: api::Song) -> Song {
            return Song {
                id: json.id,
                user: json.user.id,
                artwork_url: json.artwork.replace("-large", "-t500x500"),
                url: json.url,
                title: json.title,
                media: Media::from(json.media),
            };
        }
    }

    pub struct SoundCloud {
        songs: HashMap<u64, Song>,
        song_urls: HashMap<String, u64>,
        users: HashMap<u64, User>,
        user_urls: HashMap<String, u64>,
    }

    impl SoundCloud {
        pub async fn song(&mut self, url: &str) -> Result<&Song, Box<dyn Error>> {
            if let Some(id) = self.song_urls.get(url) {
                return Ok(&self.songs[id]);
            }

            // Not in cache so get it now
            let json_song: api::Song = api::song(url).await?;
            // Convert to real song
            let real_song = Song::from(json_song);
            let r = self.cache_song(real_song);
            Ok(r)
        }

        fn cache_song(&mut self, s: Song) -> &Song {
            let id = s.id;
            // let url = s.url.clone();
            self.song_urls.insert(s.url.clone(), id);
            self.songs.insert(s.id, s);

            return self.songs.get(&id).unwrap();
        }
    }

    pub fn new() -> SoundCloud {
        return SoundCloud {
            songs: HashMap::new(),
            users: HashMap::new(),
            song_urls: HashMap::new(),
            user_urls: HashMap::new(),
        };
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut sc = sc::new();
    let song = sc
        .song("https://soundcloud.com/iammindsight/anotherone")
        .await?;

    let client = Client::new("LBCcHmRB8XSStWL6wKH2HPACspQlXg2P");
    let userObject = client.user(29998182).get().await?;

    println!("{:#?}", userObject);

    // let user = client.resolve("https://soundcloud.com/f1ssi0n").await?;
    // println!("{}", user);

    println!("{:#?}", song);

    Ok(())
}
