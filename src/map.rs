use std::hash::{Hash, Hasher};
use std::hash::BuildHasher;
use std::collections::hash_map::RandomState;
use spin::{Mutex, MutexGuard};
use std::default::Default;
use std::mem::swap;
use std::cmp::min;
use std::u16;
use std::borrow::Borrow;
use std::iter::{FromIterator, IntoIterator};
use table::*;

pub struct ConHashMap<K, V, H=RandomState> where K: Send + Sync, V: Send + Sync {
    tables: Vec<Mutex<Table<K, V>>>,
    hasher_factory: H,
    table_shift: u64,
    table_mask: u64,
}

impl <K, V, H> ConHashMap<K, V, H> where K: Hash + Eq + Send + Sync, V: Send + Sync, H: BuildHasher {
    pub fn new() -> ConHashMap<K, V> {
        Default::default()
    }

    //pub fn with_option(opts: Option)
}