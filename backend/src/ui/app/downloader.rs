use super::super::cache::SongCache;
use crate::sc;
use async_trait::async_trait;
use eyre::{eyre, Result};
use log::warn;
use std::sync::Arc;

pub(crate) struct Downloader {
    pub(crate) client: reqwest::Client,
    pub(crate) song_cache: Arc<SongCache>,
}

impl Downloader {
    pub(crate) fn new(song_cache: Arc<SongCache>) -> Self {
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
