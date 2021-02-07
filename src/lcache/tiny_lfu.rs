use probabilistic_collections::count_min_sketch::{CountMinSketch, CountMinStrategy};
use probabilistic_collections::cuckoo::CuckooFilter;
use std::cmp;
use std::collections::HashSet;

pub const MAX_WINDOW_SIZE: usize = 10000;

pub trait TinyLFU {
    fn estimate(&self, k: &u64) -> i64;

    fn increment(&mut self, k: &u64);

    fn reset(&mut self);

    fn clear(&mut self);
}

pub struct TinyLFUCache {
    sketcher: CountMinSketch<CountMinStrategy, u64>,
    filter: CuckooFilter<u64>,
    increments: usize,
    window_size: usize,
    actual_window: HashSet<u64>,
    previous_window: HashSet<u64>,
}

impl TinyLFUCache {
    pub fn new(window_size: usize) -> Self {
        assert_ne!(window_size, 0);
        let window_size = cmp::min(window_size, MAX_WINDOW_SIZE);
        Self {
            sketcher: CountMinSketch::from_error(0.1, 0.05),
            filter: CuckooFilter::from_entries_per_index(window_size, 0.01, 8),
            window_size,
            increments: 0,
            actual_window: HashSet::new(),
            previous_window: HashSet::new(),
        }
    }

    fn reset_sketcher(&mut self) {
        for item in self.previous_window.drain() {
            let hits = self.sketcher.count(&item);
            self.sketcher.insert(&item, -hits);
        }
        let mut tmp = HashSet::new();
        for item in self.actual_window.drain() {
            let hits = self.sketcher.count(&item);
            self.sketcher.insert(&item, -((hits / 2) + (hits % 2)));
            tmp.insert(item);
        }
        self.previous_window = tmp;
    }
}

impl TinyLFU for TinyLFUCache {
    fn estimate(&self, k: &u64) -> i64 {
        let mut hits = self.sketcher.count(k);
        if self.filter.contains(k) {
            hits += 1;
        }
        hits
    }

    fn increment(&mut self, k: &u64) {
        if self.increments >= self.window_size {
            self.reset()
        }
        if !self.filter.contains(k) {
            self.filter.insert(k);
        } else {
            self.sketcher.insert(k, 1);
            self.previous_window.remove(k);
            self.actual_window.insert(*k);
        }
        self.increments += 1;
    }

    fn reset(&mut self) {
        self.reset_sketcher();
        self.filter.clear();
        self.increments = 0;
    }

    fn clear(&mut self) {
        self.sketcher.clear();
        self.filter.clear();
        self.increments = 0;
    }
}
