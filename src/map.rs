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

fn div_ceil(n: usize, d: usize) -> usize {
    if n == 0 {
        0
    } else {
        n/d + if n % d == 0 { 1 } else { 0 }
    }
}

fn f64_to_usize(f: f64) -> Option<usize> {
    if f.is_nan() || f.is_sign_negative() || f > ::std::usize::MAX as f64 {
        None
    } else {
        Some(f as usize)
    }
}

pub struct Options<H> {
    pub capacity: usize,
    pub hasher_factory: H,
    pub concurrency: u16,
}

impl <H> Default for Options<H> where H: BuildHasher+Default {
    fn Default() -> Options<H> {
        Options {
            capacity: 0,
            hasher_factory: Default::default(),
            concurrency: 16
        }
    }
}


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

    pub fn with_options(opts: Options<H>) -> ConcHashMap<K, V, H> {
        let conc = opts.concurrency as usize;
        let partitions = conc.checked_next_power_of_two().unwrap_or((conc / 2).next_power_of_two());
        let capacity = f64_to_usize(opts.capacity as f64 / 0.92).expect("capacity overflow");
        let reserve = div_ceil(capacity, partitions);
        let mut tables = Vec::with_capacity(partitions);
        for _ in 0..partitions {
            tables.push(Mutex::new(Table::new(reserve)));
        }
        ConcHashMap {
            tables: tables,
            hasher_factory: opts.hasher_factory,
            table_shift: if partitions == 1 { 0 } else { 64 - partitions.trailing_zeros() as u64 },
            table_mask: partitions as u64 - 1
        }
    }

    #[inline(never)]
    pub fn find<'a, Q: ?Sized>(&'a self, key: &Q) -> Option<Accessor<'a, K, V>>
            where K: Borrow<Q> + Hash + Eq + Send + Sync, Q: Hash + Eq + Sync {
        let hash = self.hash(key);
        let table_idx = self.table_for(hash);
        let table = self.tables[table_idx].lock();
        match table.lookup(hash, |k| k.borrow() == key) {
            Some(idx) => Some(Accessor::new(table, idx)),
            None      => None
        }
    }

    #[inline(never)]
    pub fn find_mut<'a, Q: ?Sized>(&'a self, key: &Q) -> Option<MutAccessor<'a, K, V>>
        where K: Borrow<Q> + Hash + Eq + Send + Sync, Q: Hash + Eq + Sync {

        let hash = self.hash(key);
        let table_idx = self.table_for(hash);
        let table = self.tables[table_idx].lock();
        match table.lookup (hash, |k| k.borrow() == key) {
            Some(idx) => Some(MutAccessor::new(table, idx)),
            None => None
        }
        
    }
}