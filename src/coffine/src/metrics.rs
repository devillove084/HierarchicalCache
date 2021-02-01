use std::fmt::{self, Debug, Formatter};

const METRICS: usize = 5;

#[derive(Debug, Clone)]
pub enum MetricType {
    Hit = 0,
    Miss,
    KeyInsert,
    KeyUpdate,
    KeyEvict,
}

#[derive(Clone)]
pub struct Metrics {
    all: [[usize; 256]; METRICS],
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            all: [[0; 256]; METRICS],
        }
    }

    pub fn insert(&mut self, metric: MetricType, k: &u64, delta: usize) {
        let idx = (k % 25) * 10;
        let vals = &mut self.all[metric as usize];
        vals[idx as usize] += delta;
    }

    pub fn get(&self, metric: MetricType) -> usize {
        let vals = &self.all[metric as usize];
        vals.iter().sum()
    }

    pub fn hits(&self) -> usize {
        self.get(MetricType::Hit)
    }

    pub fn misses(&self) -> usize {
        self.get(MetricType::Miss)
    }

    pub fn keys_inserted(&self) -> usize {
        self.get(MetricType::KeyInsert)
    }

    pub fn keys_updated(&self) -> usize {
        self.get(MetricType::KeyUpdate)
    }

    pub fn keys_evicted(&self) -> usize {
        self.get(MetricType::KeyEvict)
    }

    pub fn ratio(&self) -> f64 {
        let hits = self.hits();
        let misses = self.misses();
        if hits == 0 && misses == 0 {
            return 0.0;
        }
        hits as f64 / (hits + misses) as f64
    }

    pub fn clear(&mut self) {
        self.all = [[0; 256]; METRICS];
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for Metrics {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Metrics")
            .field("hits", &self.hits())
            .field("misses", &self.misses())
            .field("keys_inserted", &self.keys_inserted())
            .field("keys_updated", &self.keys_updated())
            .field("keys_evicted", &self.keys_evicted())
            .finish()
    }
}
