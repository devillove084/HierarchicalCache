use super::store::{Item, Store};
use indexmap::map::Keys;
use std::iter::FusedIterator;

pub struct Iter<'a, K, V, S>
where
    S: Store<K, V>,
{
    store: &'a S,
    keys: Keys<'a, u64, Item<K, V>>,
}

impl<'a, K, V, S> Iter<'a, K, V, S>
where
    S: Store<K, V>,
{
    pub fn new(store: &'a S) -> Self {
        let keys = store.keys();
        Self { store, keys }
    }
}

impl<'a, K, V, S> Iterator for Iter<'a, K, V, S>
where
    S: Store<K, V>,
{
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(k) = self.keys.next() {
            if let Some(item) = self.store.get(k) {
                Some((&item.k, &item.v))
            } else {
                unreachable!()
            }
        } else {
            None
        }
    }
}

impl<'a, K, V, S> FusedIterator for Iter<'a, K, V, S> where S: Store<K, V> {}

