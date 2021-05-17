use crate::chashmap::{HashMap, iter::Keys};

use super::cache::OnEvict;
use super::tiny_lfu::TinyLFU;
use super::ttl::{Expiration, ExpirationMap};

//use indexmap::map::{IndexMap, Keys};
use log::warn;
use rand::distributions::Uniform;
use rand::{thread_rng, Rng};
use std::{borrow::BorrowMut, cmp::Ordering, convert::TryInto};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::time::{Duration, SystemTime};

pub const SAMPLES_NUM: usize = 5;

#[derive(Clone, Debug)]
pub struct SampleItem {
    pub key: u64,

    pub estimate: i64,
}

impl SampleItem {
    pub fn new(key: u64, estimate: i64) -> Self {
        Self { key, estimate }
    }
}

impl PartialOrd for SampleItem {
    fn partial_cmp(&self, other: &SampleItem) -> Option<Ordering> {
        self.estimate
            .partial_cmp(&other.estimate)
            .map(|ord| ord.then(self.key.cmp(&other.key)))
    }
}

impl Ord for SampleItem {
    fn cmp(&self, other: &SampleItem) -> Ordering {
        self.estimate
            .cmp(&other.estimate)
            .then(self.key.cmp(&other.key))
    }
}

impl PartialEq for SampleItem {
    fn eq(&self, other: &Self) -> bool {
        self.key.eq(&other.key)
    }
}

impl Eq for SampleItem {}

impl Hash for SampleItem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

#[derive(Debug)]
pub struct Item<K, V> {
    pub expiration_time: Option<SystemTime>,
    pub k: K,
    pub v: V,
}

// impl<K, V> Copy for Item<K, V> {}

// impl<K, V> Clone for Item<K, V> {
//     fn clone(&self) -> Self {
//         *self
//     }
// }

impl<K, V> Item<K, V> {
    pub fn new(k: K, v: V) -> Self {
        Self {
            expiration_time: None,
            k,
            v,
        }
    }
}

impl<K, V> Deref for Item<K, V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.v
    }
}

pub trait Store<K, V>: Iterator {
    fn capacity(&self) -> usize;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn room_left(&self) -> usize;

    fn contains(&self, k: &u64) -> bool;

    fn keys(&self) -> Keys<u64, Item<K, V>>;

    fn get(&self, k: &u64) -> Option<&Item<K, V>>;

    fn get_mut(&mut self, k: &u64) -> Option<&Item<K, V>>;

    fn insert(&mut self, k: u64, item: Item<K, V>) -> Option<Item<K, V>> {
        self.insert_with_ttl(k, item, Duration::from_secs(0))
    }

    fn insert_with_ttl(
        &mut self,
        k: u64,
        item: Item<K, V>,
        expiration: Duration,
    ) -> Option<Item<K, V>>;

    fn remove(&mut self, k: &u64) -> Option<&Item<K, V>>;

    fn cleanup<E>(&mut self, on_evict: &Option<E>)
    where
        E: OnEvict<K, V>;

    fn clear(&mut self);

    fn sample(&self, admit: &impl TinyLFU) -> Option<SampleItem>;
}

pub struct Storage_plus<K, V> {
    data: HashMap<u64, Item<K, V>>,
    expiration_map: ExpirationMap,
    capacity: usize,
}

unsafe impl<K, V> Send for Storage_plus<K, V> {}
unsafe impl<K, V> Sync for Storage_plus<K, V> {}

impl<K, V> Storage_plus<K, V> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            data: HashMap::new(),
            expiration_map: ExpirationMap::new(),
        }
    }
}

impl<K, V> Iterator  for Storage_plus<K, V> {
    type Item = Item<K, V>;
    
    fn next(&mut self) -> Option<Self::Item> {
        unimplemented!()
    }
}

impl<K, V> Store<K, V> for Storage_plus<K, V> 
where 
K: 'static + Sync + Send + Clone + Hash + Ord,
V: 'static + Sync + Send, {
    fn capacity(&self) -> usize {
        self.capacity
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn room_left(&self) -> usize {
        self.capacity() - self.len()
    }

    fn contains(&self, k: &u64) -> bool {
        self.get(k).is_some()
    }

    fn keys(&self) -> Keys<u64, Item<K, V>> {
        let g = self.data.guard();
        let result = self.data.keys(&g);
        result
    }

    fn get(&self, k: &u64) -> Option<&Item<K, V>> {
        let g = self.data.guard();
        if let Some(item) = self.data.get(k, &g) {
            if let Some(expiration_time) = &item.expiration_time {
                if SystemTime::now().gt(expiration_time) {
                    None
                } else {
                    Some(item)
                }
            }else {
                Some(item)
            } 
        }else {
            None
        }
    }

    fn get_mut(&mut self, k: &u64) -> Option<&Item<K, V>> {
        let g = self.data.guard();
        if let Some(item) = self.data.get_mut(k, &g) {
            if let Some(expiration_time) = &item.expiration_time {
                if SystemTime::now().gt(expiration_time) {
                    None
                } else {
                    Some(item)
                }
            } else {
                Some(item.borrow_mut())
            }
        } else {
            None
        }
    }

    fn insert_with_ttl(&mut self, k: u64, mut item: Item<K, V>, expiration: Duration) -> Option<Item<K, V>> {
        let g = self.data.guard();
        let old_item = if let Some(old_item) = self.data.remove(&k, &g) {
            if let Some(expiration_time) = &old_item.expiration_time {
                self.expiration_map.remove(&k, expiration_time);
            }
            Some(old_item)
        }else {
            None
        };

        item.expiration_time = self.expiration_map.insert(k, expiration);
        self.data.insert(k, item, &g);
        let i = old_item.unwrap();
        Some(*i)
    }

    fn remove(&mut self, k: &u64) -> Option<&Item<K, V>> {
        let g = self.data.guard();
        // if let Some(item) = self.data.remove(&k, &g) {
        //     if let Some(expiration_time) = &item.expiration_time {
        //         self.expiration_map.remove(k, expiration_time);
        //     }
        //     let i = item;
        //     //Some(*i.borrow_mut())
        //     Some(())
        // }else {
        //     None
        // }
        self.data.remove(k, &g)

    }

    fn cleanup<E>(&mut self, on_evict: &Option<E>)
    where
            E: OnEvict<K, V> {
        
    }

    fn clear(&mut self) {
        self.expiration_map.clear();
        let g = self.data.guard();
        self.data.clear(&g);
    }

    fn sample(&self, admit: &impl TinyLFU) -> Option<SampleItem> {
        if self.is_empty() {
            return None;
        }

        let items_range = Uniform::new(0_usize, self.len());
        let mut generator = thread_rng().sample_iter(items_range);
        let mut result: Option<SampleItem> = None;
        for _ in 0 .. SAMPLES_NUM {
            let index: u64 = generator.next().unwrap().try_into().unwrap();
            let g = self.data.guard();
            let (k, _) = self.data.get_key_value(&index, &g).expect("Sample item");
            let estimate = admit.estimate(&k);
            let sample = SampleItem::new(*k, estimate);
            if let Some(current) = &result {
                if sample.estimate.lt(&current.estimate) {
                    result = Some(sample);
                }
            } else {
                result = Some(sample)
            }
        }
        result
    }
}


// pub struct Storage<K, V> {
//     data: IndexMap<u64, Item<K, V>>,
//     expiration_map: ExpirationMap,
//     capacity: usize,
// }

// impl<K, V> Storage<K, V> {
//     pub fn with_capacity(capacity: usize) -> Self {
//         Self {
//             capacity,
//             data: HashMap::new(),
//             expiration_map: ExpirationMap::new(),
//         }
//     }
// }

// impl<K, V> Iterator for Storage<K, V> {
//     type Item = Item<K, V>;

//     fn next(&mut self) -> Option<Self::Item> {
//         unimplemented!()
//         //TODO: 实现主要迭代过程
//     }
// }

// impl<K, V> Store<K, V> for Storage<K, V> {
//     fn capacity(&self) -> usize {
//         self.capacity
//     }

//     fn len(&self) -> usize {
//         self.data.len()
//     }

//     fn room_left(&self) -> usize {
//         self.capacity() - self.len()
//     }

//     fn contains(&self, k: &u64) -> bool {
//         self.get(k).is_some()
//     }

//     fn keys(&self) -> Keys<u64, Item<K, V>> {
//         self.data.keys()
//     }

//     fn get(&self, k: &u64) -> Option<&Item<K, V>> {
//         if let Some(item) = self.data.get(k) {
//             if let Some(expiration_time) = &item.expiration_time {
//                 if SystemTime::now().gt(expiration_time) {
//                     None
//                 } else {
//                     Some(item)
//                 }
//             } else {
//                 Some(item)
//             }
//         } else {
//             None
//         }
//     }

//     fn get_mut(&mut self, k: &u64) -> Option<&mut Item<K, V>> {
//         if let Some(item) = self.data.get_mut(k) {
//             if let Some(expiration_time) = &item.expiration_time {
//                 if SystemTime::now().gt(expiration_time) {
//                     None
//                 } else {
//                     Some(item)
//                 }
//             } else {
//                 Some(item)
//             }
//         } else {
//             None
//         }
//     }

//     fn insert_with_ttl(
//         &mut self,
//         k: u64,
//         mut item: Item<K, V>,
//         expiration: Duration,
//     ) -> Option<Item<K, V>> {
//         let old_item = if let Some(old_item) = self.data.remove(&k) {
//             if let Some(expiration_time) = &old_item.expiration_time {
//                 self.expiration_map.remove(&k, expiration_time);
//             }
//             Some(old_item)
//         } else {
//             None
//         };
//         item.expiration_time = self.expiration_map.insert(k, expiration);
//         self.data.insert(k, item);
//         old_item
//     }

//     fn remove(&mut self, k: &u64) -> Option<Item<K, V>> {
//         if let Some(item) = self.data.remove(k) {
//             if let Some(expiration_time) = &item.expiration_time {
//                 self.expiration_map.remove(k, expiration_time);
//             }
//             Some(item)
//         } else {
//             None
//         }
//     }

//     fn cleanup<E>(&mut self, on_evict: &Option<E>)
//     where
//         E: OnEvict<K, V>,
//     {
//         let now = SystemTime::now();
//         let keys = self.expiration_map.cleanup(&now);
//         for k in keys {
//             if let Some(item) = self.data.get(&k) {
//                 if let Some(expiration_time) = &item.expiration_time {
//                     if now.lt(expiration_time) {
//                         warn!("Expiration map contains invalid expiration time for item!");
//                         continue;
//                     }
//                 } else {
//                     warn!("Expiration map contains item without expiration time!");
//                     continue;
//                 }
//             } else {
//                 warn!("Expiration map contains invalid item!");
//                 continue;
//             }
//             let item = self.remove(&k).unwrap();
//             if let Some(on_evict) = on_evict {
//                 on_evict.evict(&item.k, &item.v);
//             }
//         }
//     }

//     fn clear(&mut self) {
//         self.expiration_map.clear();
//         self.data.clear();
//     }

//     fn sample(&self, admit: &impl TinyLFU) -> Option<SampleItem> {
//         if self.is_empty() {
//             return None;
//         }
//         let items_range = Uniform::new(0_usize, self.len());
//         let mut generator = thread_rng().sample_iter(items_range);
//         let mut result: Option<SampleItem> = None;
//         for _ in 0..SAMPLES_NUM {
//             let index = generator.next().unwrap();
//             let (k, _) = self.data.get_index(index).expect("sample item");
//             let estimate = admit.estimate(&k);
//             let sample = SampleItem::new(*k, estimate);
//             if let Some(current) = &result {
//                 if sample.estimate.lt(&current.estimate) {
//                     result = Some(sample);
//                 }
//             } else {
//                 result = Some(sample)
//             }
//         }
//         result
//     }
// }
