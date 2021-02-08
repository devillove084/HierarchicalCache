use super::epoch::Guard;
use super::iter::Keys;
use super::HashMap;
use std::borrow::Borrow;
use std::fmt::{self, Debug, Formatter};
use std::hash::{BuildHasher, Hash};
use std::iter::FromIterator;

pub struct HashSet<T, S = super::DefaultHashBuilder> {
    pub(crate) map: HashMap<T, (), S>,
}

impl<T> HashSet<T, super::DefaultHashBuilder> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, super::DefaultHashBuilder::default())
    }
}

impl<T, S> Default for HashSet<T, S>
where
    S: Default,
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}

impl<T, S> HashSet<T, S> {
    pub fn with_hasher(hash_builder: S) -> Self {
        Self {
            map: HashMap::with_hasher(hash_builder),
        }
    }

    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        Self {
            map: HashMap::with_capacity_and_hasher(capacity, hash_builder),
        }
    }

    pub fn guard(&self) -> super::epoch::Guard {
        self.map.guard()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter<'g>(&'g self, guard: &'g Guard) -> Keys<'g, T, ()> {
        self.map.keys(guard)
    }
}

impl<T, S> HashSet<T, S>
where
    T: Hash + Ord,
    S: BuildHasher,
{
    #[inline]
    pub fn contains<'g, Q>(&self, value: &Q, guard: &'g Guard) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.map.contains_key(value, guard)
    }

    pub fn get<'g, Q>(&'g self, value: &Q, guard: &'g Guard) -> Option<&'g T>
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.map.get_key_value(value, guard).map(|(k, _)| k)
    }

    pub fn is_disjoint(
        &self,
        other: &HashSet<T, S>,
        our_guard: &Guard,
        their_guard: &Guard,
    ) -> bool {
        for value in self.iter(our_guard) {
            if other.contains(&value, their_guard) {
                return false;
            }
        }

        true
    }

    pub fn is_subset(&self, other: &HashSet<T, S>, our_guard: &Guard, their_guard: &Guard) -> bool {
        for value in self.iter(our_guard) {
            if !other.contains(&value, their_guard) {
                return false;
            }
        }

        true
    }

    pub fn is_superset(
        &self,
        other: &HashSet<T, S>,
        our_guard: &Guard,
        their_guard: &Guard,
    ) -> bool {
        other.is_subset(self, their_guard, our_guard)
    }

    pub(crate) fn guarded_eq(&self, other: &Self, our_guard: &Guard, their_guard: &Guard) -> bool {
        self.map.guarded_eq(&other.map, our_guard, their_guard)
    }
}

impl<T, S> HashSet<T, S>
where
    T: 'static + Sync + Send + Clone + Hash + Ord,
    S: BuildHasher,
{
    pub fn insert(&self, value: T, guard: &Guard) -> bool {
        let old = self.map.insert(value, (), guard);
        old.is_none()
    }

    pub fn remove<Q>(&self, value: &Q, guard: &Guard) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        let removed = self.map.remove(value, guard);
        removed.is_some()
    }

    pub fn take<'g, Q>(&'g self, value: &Q, guard: &'g Guard) -> Option<&'g T>
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.map.remove_entry(value, guard).map(|(k, _)| k)
    }

    pub fn retain<F>(&self, mut f: F, guard: &Guard)
    where
        F: FnMut(&T) -> bool,
    {
        self.map.retain(|value, ()| f(value), guard)
    }
}

impl<T, S> HashSet<T, S>
where
    T: Clone + Ord,
{
    pub fn clear(&self, guard: &Guard) {
        self.map.clear(guard)
    }

    pub fn reserve(&self, additional: usize, guard: &Guard) {
        self.map.reserve(additional, guard)
    }
}

impl<T, S> PartialEq for HashSet<T, S>
where
    T: Ord + Hash,
    S: BuildHasher,
{
    fn eq(&self, other: &Self) -> bool {
        self.map == other.map
    }
}

impl<T, S> Eq for HashSet<T, S>
where
    T: Ord + Hash,
    S: BuildHasher,
{
}

impl<T, S> fmt::Debug for HashSet<T, S>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let guard = self.guard();
        f.debug_set().entries(self.iter(&guard)).finish()
    }
}

impl<T, S> Extend<T> for &HashSet<T, S>
where
    T: 'static + Sync + Send + Clone + Hash + Ord,
    S: BuildHasher,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        Extend::extend(&mut &self.map, iter.into_iter().map(|v| (v, ())))
    }
}

impl<'a, T, S> Extend<&'a T> for &HashSet<T, S>
where
    T: 'static + Sync + Send + Copy + Hash + Ord,
    S: BuildHasher,
{
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        Extend::extend(&mut &self.map, iter.into_iter().map(|&v| (v, ())))
    }
}

impl<T, S> FromIterator<T> for HashSet<T, S>
where
    T: 'static + Sync + Send + Clone + Hash + Ord,
    S: BuildHasher + Default,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            map: HashMap::from_iter(iter.into_iter().map(|v| (v, ()))),
        }
    }
}

impl<'a, T, S> FromIterator<&'a T> for HashSet<T, S>
where
    T: 'static + Sync + Send + Copy + Hash + Ord,
    S: BuildHasher + Default,
{
    fn from_iter<I: IntoIterator<Item = &'a T>>(iter: I) -> Self {
        Self {
            map: HashMap::from_iter(iter.into_iter().map(|&v| (v, ()))),
        }
    }
}

impl<T, S> Clone for HashSet<T, S>
where
    T: 'static + Sync + Send + Clone + Hash + Ord,
    S: BuildHasher + Clone,
{
    fn clone(&self) -> HashSet<T, S> {
        Self {
            map: self.map.clone(),
        }
    }
}
