#![allow(dead_code)]

use super::iter::*;
use super::{GuardRef, HashMap, TryInsertError};
use crossbeam_epoch::Guard;
use std::borrow::Borrow;
use std::fmt::{self, Debug, Formatter};
use std::hash::{BuildHasher, Hash};
use std::ops::Index;

pub struct HashMapRef<'map, K, V, S = crate::DefaultHashBuilder> {
    pub(crate) map: &'map HashMap<K, V, S>,
    guard: GuardRef<'map>,
}

impl<K, V, S> HashMap<K, V, S> {
    pub fn pin(&self) -> HashMapRef<'_, K, V, S> {
        HashMapRef {
            guard: GuardRef::Owned(self.guard()),
            map: &self,
        }
    }

    pub fn with_guard<'g>(&'g self, guard: &'g Guard) -> HashMapRef<'g, K, V, S> {
        HashMapRef {
            map: &self,
            guard: GuardRef::Ref(guard),
        }
    }
}

impl<K, V, S> HashMapRef<'_, K, V, S> {
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        self.map.iter(&self.guard)
    }

    pub fn keys(&self) -> Keys<'_, K, V> {
        self.map.keys(&self.guard)
    }

    pub fn values(&self) -> Values<'_, K, V> {
        self.map.values(&self.guard)
    }
}

impl<K, V, S> HashMapRef<'_, K, V, S>
where
    K: Clone + Ord,
{
    pub fn reserve(&self, additional: usize) {
        self.map.reserve(additional, &self.guard)
    }
}

impl<K, V, S> HashMapRef<'_, K, V, S>
where
    K: Hash + Ord,
    S: BuildHasher,
{
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.map.contains_key(key, &self.guard)
    }

    #[inline]
    pub fn get<'g, Q>(&'g self, key: &Q) -> Option<&'g V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.map.get(key, &self.guard)
    }

    #[inline]
    pub fn get_mut<'g, Q>(&'g self, key: &Q) -> Option<&'g V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.map.get(key, &self.guard)
    }

    #[inline]
    pub fn get_key_value<'g, Q>(&'g self, key: &Q) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.map.get_key_value(key, &self.guard)
    }
}

impl<K, V, S> HashMapRef<'_, K, V, S>
where
    K: Clone + Ord,
{
    pub fn clear(&self) {
        self.map.clear(&self.guard);
    }
}

impl<K, V, S> HashMapRef<'_, K, V, S>
where
    K: 'static + Sync + Send + Clone + Hash + Ord,
    V: 'static + Sync + Send,
    S: BuildHasher,
{
    pub fn insert(&self, key: K, value: V) -> Option<&'_ V> {
        self.map.insert(key, value, &self.guard)
    }

    #[inline]
    pub fn try_insert(&self, key: K, value: V) -> Result<&'_ V, TryInsertError<'_, V>> {
        self.map.try_insert(key, value, &self.guard)
    }

    pub fn compute_if_present<'g, Q, F>(&'g self, key: &Q, remapping_function: F) -> Option<&'g V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
        F: FnOnce(&K, &V) -> Option<V>,
    {
        self.map
            .compute_if_present(key, remapping_function, &self.guard)
    }

    pub fn remove<'g, Q>(&'g self, key: &Q) -> Option<&'g V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.map.remove(key, &self.guard)
    }

    pub fn remove_entry<'g, Q>(&'g self, key: &Q) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.map.remove_entry(key, &self.guard)
    }

    pub fn retain<F>(&self, f: F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        self.map.retain(f, &self.guard);
    }

    pub fn retain_force<F>(&self, f: F)
    where
        F: FnMut(&K, &V) -> bool,
    {
        self.map.retain_force(f, &self.guard);
    }
}

impl<'g, K, V, S> IntoIterator for &'g HashMapRef<'_, K, V, S> {
    type IntoIter = Iter<'g, K, V>;
    type Item = (&'g K, &'g V);

    fn into_iter(self) -> Self::IntoIter {
        self.map.iter(&self.guard)
    }
}

impl<K, V, S> Debug for HashMapRef<'_, K, V, S>
where
    K: Debug,
    V: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self).finish()
    }
}

impl<K, V, S> Clone for HashMapRef<'_, K, V, S> {
    fn clone(&self) -> Self {
        self.map.pin()
    }
}

impl<K, V, S> PartialEq for HashMapRef<'_, K, V, S>
where
    K: Hash + Ord,
    V: PartialEq,
    S: BuildHasher,
{
    fn eq(&self, other: &Self) -> bool {
        self.map.guarded_eq(&other.map, &self.guard, &other.guard)
    }
}

impl<K, V, S> PartialEq<HashMap<K, V, S>> for HashMapRef<'_, K, V, S>
where
    K: Hash + Ord,
    V: PartialEq,
    S: BuildHasher,
{
    fn eq(&self, other: &HashMap<K, V, S>) -> bool {
        self.map.guarded_eq(&other, &self.guard, &other.guard())
    }
}

impl<K, V, S> PartialEq<HashMapRef<'_, K, V, S>> for HashMap<K, V, S>
where
    K: Hash + Ord,
    V: PartialEq,
    S: BuildHasher,
{
    fn eq(&self, other: &HashMapRef<'_, K, V, S>) -> bool {
        self.guarded_eq(&other.map, &self.guard(), &other.guard)
    }
}

impl<K, V, S> Eq for HashMapRef<'_, K, V, S>
where
    K: Hash + Ord,
    V: Eq,
    S: BuildHasher,
{
}

impl<K, Q, V, S> Index<&'_ Q> for HashMapRef<'_, K, V, S>
where
    K: Hash + Ord + Borrow<Q>,
    Q: ?Sized + Hash + Ord,
    S: BuildHasher,
{
    type Output = V;

    fn index(&self, key: &Q) -> &V {
        self.get(key).expect("no entry found for key")
    }
}
