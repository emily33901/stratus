use std::{
    cell::RefCell,
    cmp,
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use iced::{image::Handle, Image};
use parking_lot::{MappedRwLockReadGuard, Mutex, RwLock, RwLockReadGuard};

use log::{info, warn};

use crate::sc;

pub struct Cache<K, V> {
    loaded: RwLock<HashMap<K, V>>,
    needs_loading: Mutex<Vec<K>>,
    blacklist: RwLock<HashSet<K>>,
}

impl<K, V> Default for Cache<K, V>
where
    K: cmp::Eq + core::hash::Hash + Clone + Debug,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Cache<K, V>
where
    K: cmp::Eq + core::hash::Hash + Clone + Debug,
{
    pub fn new() -> Self {
        Self {
            loaded: RwLock::new(HashMap::new()),
            needs_loading: Mutex::new(Vec::new()),
            blacklist: RwLock::new(HashSet::new()),
        }
    }

    pub fn try_get(&self, key: &K) -> Option<MappedRwLockReadGuard<V>> {
        let read = self.loaded.read();
        RwLockReadGuard::try_map(read, |read| match read.get(&key) {
            None => {
                if self.blacklist.read().contains(&key) {
                    warn!("{:?} is already blacklisted. NOT trying again", key);
                } else {
                    self.blacklist.write().insert(key.clone());
                    self.needs_loading.lock().push(key.clone());
                }
                None
            }
            some => some,
        })
        .ok()
    }

    pub fn write(&self, key: K, value: V) {
        info!("Wrote {:?}", key);
        self.loaded.write().insert(key, value);
    }

    pub fn needs_loading(&self) -> Vec<K> {
        self.needs_loading.lock().drain(..).collect::<Vec<_>>()
    }
}

pub type ImageCache = Cache<String, Handle>;

impl ImageCache {
    pub fn image_for_song(&self, song: &sc::Song) -> Option<Image> {
        let url = song.artwork.as_ref()?.replace("-large", "-t500x500");

        let handle = self.try_get(&url)?;
        Some(Image::new(handle.clone()))
    }
}

pub type SongCache = Cache<sc::Object, sc::Song>;
