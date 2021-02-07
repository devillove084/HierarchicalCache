#![allow(dead_code)]

use super::iter::Iter;
use super::metrics::{MetricType, Metrics};
use super::store::{Item, SampleItem, Storage, Store};
use super::tiny_lfu::{TinyLFU, TinyLFUCache, MAX_WINDOW_SIZE};

use probabilistic_collections::SipHasherBuilder;
use std::hash::{BuildHasher, Hash, Hasher};
use std::marker::PhantomData;
use std::sync::Mutex;
use std::time::Duration;

pub trait OnEvict<K, V> {
    fn evict(&self, k: &K, v: &V);
}

pub struct VoidEvict<K, V> {
    _k: PhantomData<K>,
    _v: PhantomData<V>,
}

impl<K, V> OnEvict<K, V> for VoidEvict<K, V> {
    fn evict(&self, _k: &K, _v: &V) {}
}

pub struct Cache<
    K,
    V,
    E = VoidEvict<K, V>,
    S = Storage<K, V>,
    A = TinyLFUCache,
    H = SipHasherBuilder,
> where
    K: Eq + Hash,
    E: OnEvict<K, V>,
    S: Store<K, V>,
    A: TinyLFU,
    H: BuildHasher,
{
    hasher_builder: H,
    pub(crate) store: S,
    admit: Mutex<A>,
    on_evict: Option<E>,
    metrics: Mutex<Option<Metrics>>,
    _k: PhantomData<K>,
    _v: PhantomData<V>,
}

impl<K: Eq + Hash, V> Cache<K, V> {
    pub fn new(capacity: usize) -> Self {
        Self::with_window_size(capacity, MAX_WINDOW_SIZE)
    }

    pub fn with_window_size(capacity: usize, window_size: usize) -> Self {
        assert_ne!(window_size, 0);
        assert!(window_size <= 10_000);
        assert_ne!(capacity, 0);
        Self {
            _k: PhantomData::default(),
            _v: PhantomData::default(),
            metrics: Mutex::new(None),
            on_evict: None,
            admit: Mutex::new(TinyLFUCache::new(window_size)),
            store: Storage::with_capacity(capacity),
            hasher_builder: SipHasherBuilder::from_entropy(),
        }
    }
}

impl<K, V, E> Cache<K, V, E>
where
    K: Eq + Hash,
    E: OnEvict<K, V>,
{
    pub fn with_on_evict(capacity: usize, on_evict: E) -> Self {
        Self::with_on_evict_and_window_size(capacity, on_evict, MAX_WINDOW_SIZE)
    }

    pub fn with_on_evict_and_window_size(capacity: usize, on_evict: E, window_size: usize) -> Self {
        assert_ne!(window_size, 0);
        assert!(window_size <= 10_000);
        assert_ne!(capacity, 0);
        Self {
            _k: PhantomData::default(),
            _v: PhantomData::default(),
            metrics: Mutex::new(None),
            on_evict: Some(on_evict),
            admit: Mutex::new(TinyLFUCache::new(window_size)),
            store: Storage::with_capacity(capacity),
            hasher_builder: SipHasherBuilder::from_entropy(),
        }
    }
}

impl<K, V, E, S, A, H> Cache<K, V, E, S, A, H>
where
    K: Eq + Hash,
    E: OnEvict<K, V>,
    S: Store<K, V>,
    A: TinyLFU,
    H: BuildHasher,
{
    fn key_hash(&self, k: &K) -> u64 {
        let mut hasher = self.hasher_builder.build_hasher();
        k.hash(&mut hasher);
        hasher.finish()
    }

    fn remove_victim(&mut self, victim: Option<SampleItem>) {
        if let Some(victim) = victim {
            if let Some(removed) = self.store.remove(&victim.key) {
                let k = self.key_hash(&removed.k);
                let mut metrics = self.metrics.lock().unwrap();
                if let Some(metrics) = &mut *metrics {
                    metrics.insert(MetricType::KeyEvict, &k, 1);
                }
                if let Some(on_evict) = &self.on_evict {
                    on_evict.evict(&removed.k, &removed.v);
                }
            }
        }
    }

    fn insert_item_with_ttl(
        &mut self,
        k: u64,
        item: Item<K, V>,
        expiration: Duration,
    ) -> Option<V> {
        if let Some(old_item) = self.store.insert_with_ttl(k, item, expiration) {
            Some(old_item.v)
        } else {
            None
        }
    }

    fn can_be_insert(&mut self, k: &u64) -> Result<Option<SampleItem>, Option<SampleItem>> {
        if self.store.contains(k) {
            let mut metrics = self.metrics.lock().unwrap();
            if let Some(metrics) = &mut *metrics {
                metrics.insert(MetricType::KeyUpdate, &k, 1);
            }
            return Ok(None);
        }

        if self.store.room_left() > 0 {
            return Ok(None);
        }

        let admit = self.admit.lock().unwrap();
        let incoming_estimate = admit.estimate(&k);

        let victim = self.store.sample(&*admit);
        if let Some(victim) = victim {
            if incoming_estimate < victim.estimate {
                Err(Some(victim))
            } else {
                Ok(Some(victim))
            }
        } else {
            unreachable!()
        }
    }

    pub fn with_metrics(self) -> Self {
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.replace(Metrics::new());
        }
        self
    }

    pub fn capacity(&self) -> usize {
        self.store.capacity()
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    pub fn room_left(&self) -> usize {
        self.store.room_left()
    }

    pub fn contains(&self, k: &K) -> bool {
        let k = self.key_hash(k);
        self.store.contains(&k)
    }

    pub fn get(&self, k: &K) -> Option<&V> {
        let k = self.key_hash(k);
        {
            let mut admit = self.admit.lock().unwrap();
            admit.increment(&k);
        }
        let result = if let Some(item) = self.store.get(&k) {
            Some(&item.v)
        } else {
            None
        };
        let found = result.is_some();
        {
            let mut metrics = self.metrics.lock().unwrap();
            if let Some(metrics) = &mut *metrics {
                if found {
                    metrics.insert(MetricType::Hit, &k, 1);
                } else {
                    metrics.insert(MetricType::Miss, &k, 1);
                }
            }
        }
        result
    }

    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        let k = self.key_hash(k);
        {
            let mut admit = self.admit.lock().unwrap();
            admit.increment(&k);
        }
        let result = if let Some(item) = self.store.get_mut(&k) {
            Some(&mut item.v)
        } else {
            None
        };
        let found = result.is_some();
        {
            let mut metrics = self.metrics.lock().unwrap();
            if let Some(metrics) = &mut *metrics {
                if found {
                    metrics.insert(MetricType::Hit, &k, 1);
                } else {
                    metrics.insert(MetricType::Miss, &k, 1);
                }
            }
        }
        result
    }

    pub fn insert(&mut self, k: K, v: V) -> Result<Option<V>, Option<()>> {
        self.insert_with_ttl(k, v, Duration::from_secs(0))
    }

    pub fn insert_with_ttl(
        &mut self,
        k: K,
        v: V,
        expiration: Duration,
    ) -> Result<Option<V>, Option<()>> {
        self.store.cleanup(&self.on_evict);

        let key_hash = self.key_hash(&k);
        let item = Item::new(k, v);

        match self.can_be_insert(&key_hash) {
            Ok(victim) => {
                {
                    let mut admit = self.admit.lock().unwrap();
                    admit.increment(&key_hash);
                }
                self.remove_victim(victim);
                {
                    let mut metrics = self.metrics.lock().unwrap();
                    if let Some(metrics) = &mut *metrics {
                        metrics.insert(MetricType::KeyInsert, &key_hash, 1);
                    }
                }
                Ok(self.insert_item_with_ttl(key_hash, item, expiration))
            }
            Err(victim) => {
                self.remove_victim(victim);
                Err(Some(()))
            }
        }
    }

    pub fn remove(&mut self, k: &K) -> Option<V> {
        let k = self.key_hash(k);
        if let Some(item) = self.store.remove(&k) {
            Some(item.v)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.store.clear();
        {
            let mut admit = self.admit.lock().unwrap();
            admit.clear();
        }
        {
            let mut metrics = self.metrics.lock().unwrap();
            if let Some(metrics) = &mut *metrics {
                metrics.clear();
            }
        }
    }

    pub fn metrics(&self) -> Option<Metrics> {
        let metrics = self.metrics.lock().unwrap();
        if let Some(metrics) = &*metrics {
            Some(metrics.clone())
        } else {
            None
        }
    }

    pub fn iter(&self) -> Iter<K, V, S> {
        Iter::new(&self.store)
    }
}
