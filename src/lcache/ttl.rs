use std::collections::{BTreeMap, HashSet};
use std::ops::Add;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn storage_bucket(expiration_time: &SystemTime) -> u64 {
    expiration_time
        .duration_since(UNIX_EPOCH)
        .expect("Unix Epoch")
        .as_secs()
}

pub trait Expiration {
    fn insert(&mut self, k: u64, expiration: Duration) -> Option<SystemTime>;

    fn update(
        &mut self,
        k: u64,
        expiration_time: &SystemTime,
        new_expiration: Duration,
    ) -> Option<SystemTime>;

    fn remove(&mut self, k: &u64, expiration_time: &SystemTime) -> bool;

    fn cleanup(&mut self, now: &SystemTime) -> HashSet<u64>;

    fn clear(&mut self);

    fn is_empty(&self) -> bool;
}

#[derive(Clone, Debug)]
pub struct ExpirationMap {
    buckets: BTreeMap<u64, HashSet<u64>>,
}

impl ExpirationMap {
    pub fn new() -> Self {
        Self {
            buckets: BTreeMap::new(),
        }
    }
}

impl Expiration for ExpirationMap {
    fn insert(&mut self, k: u64, expiration: Duration) -> Option<SystemTime> {
        if expiration.as_secs() == 0 {
            return None;
        }
        let expiration_time = SystemTime::now().add(expiration);
        let bucket_num = storage_bucket(&expiration_time);
        if let Some(bucket) = self.buckets.get_mut(&bucket_num) {
            bucket.insert(k);
        } else {
            let mut bucket = HashSet::new();
            bucket.insert(k);
            self.buckets.insert(bucket_num, bucket);
        }
        Some(expiration_time)
    }

    fn update(
        &mut self,
        k: u64,
        expiration_time: &SystemTime,
        new_expiration: Duration,
    ) -> Option<SystemTime> {
        self.remove(&k, expiration_time);
        self.insert(k, new_expiration)
    }

    fn remove(&mut self, k: &u64, expiration_time: &SystemTime) -> bool {
        let old_bucket_num = storage_bucket(expiration_time);
        if let Some(bucket) = self.buckets.get_mut(&old_bucket_num) {
            bucket.remove(k)
        } else {
            false
        }
    }

    fn cleanup(&mut self, now: &SystemTime) -> HashSet<u64> {
        let now = storage_bucket(now) + 1;
        let mut result = HashSet::new();
        let mut buckets = Vec::new();
        for (id, _) in self.buckets.range(..now) {
            buckets.push(*id)
        }
        for bucket in buckets {
            for item in self.buckets.remove(&bucket).unwrap() {
                result.insert(item);
            }
        }
        result
    }

    fn clear(&mut self) {
        self.buckets.clear();
    }

    fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}
