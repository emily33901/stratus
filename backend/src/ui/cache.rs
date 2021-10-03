use std::{
    cell::RefCell,
    cmp,
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use log::{info, warn};

pub struct Cache<K, V> {
    loaded: HashMap<K, V>,
    needs_loading: RefCell<Vec<K>>,
    blacklist: RefCell<HashSet<K>>,
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
            loaded: HashMap::new(),
            needs_loading: RefCell::new(Vec::new()),
            blacklist: RefCell::new(HashSet::new()),
        }
    }

    pub fn try_get(&self, key: &K) -> Option<&V> {
        match { self.loaded.get(&key) } {
            None => {
                if self.blacklist.borrow().contains(&key) {
                    warn!("{:?} is already blacklisted. NOT trying again", key);
                } else {
                    self.blacklist.borrow_mut().insert(key.clone());
                    self.needs_loading.borrow_mut().push(key.clone());
                }
                None
            }
            some => some,
        }
    }

    pub fn write(&mut self, key: K, value: V) {
        info!("Wrote {:?}", key);
        self.loaded.insert(key, value);
    }

    pub fn needs_loading(&self) -> Vec<K> {
        self.needs_loading.take()
    }
}
