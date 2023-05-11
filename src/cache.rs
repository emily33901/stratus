use eyre::{eyre, Result};
use futures::{future::BoxFuture, Future};
use std::{
    cmp,
    collections::{HashMap, HashSet},
    fmt::Debug,
    sync::Arc,
};
use tokio::sync::{Mutex, RwLock};

pub struct Cache<K, V> {
    values: RwLock<HashMap<K, Arc<Mutex<Option<Result<Arc<V>>>>>>>,
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
            values: Default::default(),
        }
    }

    pub async fn get<F>(&self, key: &K, f: F) -> Result<Arc<V>>
    where
        F: Future<Output = Result<Arc<V>>> + Send,
    {
        {
            let values = self.values.read().await;
            if let Some(v) = values.get(key) {
                let v = v.lock().await;
                let r = v.as_ref().unwrap();
                return r
                    .as_ref()
                    .map(|a| a.clone())
                    .map_err(|err| eyre!("Cache value Error: {err}"));
            }
        }

        let mut values = self.values.write().await;
        values.insert(key.clone(), Arc::default());
        let mutex = values.get(key).unwrap().clone();
        // Lock interior mutex before dropping lock on values

        let mut mutex = mutex.lock().await;
        drop(values);

        // Now that we hold lock on the values mutex, but not the hashmap, we can eval f
        let v = f.await;
        *mutex = Some(
            v.as_ref()
                .map(|v| v.clone())
                .map_err(|err| eyre!("Cache value Error: {err}")),
        );
        v
    }

    pub async fn write(&self, key: K, v: Arc<V>) {
        self.values
            .write()
            .await
            .insert(key, Arc::new(Mutex::new(Some(Ok(v)))));
    }
}
