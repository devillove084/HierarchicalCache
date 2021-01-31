use crate::iter::*;
use crate::{GuardRef, HashSet};
use crossbeam_epoch::Guard;
use std::borrow::Borrow;
use std::fmt::{self, Debug, Formatter};
use std::hash::{BuildHasher, Hash};

pub struct HashSetRef<'set, T, S = crate::DefaultHashBuilder> {
    pub(crate) set: &'set HashSet<T, S>,
    guard: GuardRef<'set>,
}

impl<T, S> HashSet<T, S> {
    pub fn pin(&self) -> HashSetRef<'_, T, S> {
        HashSetRef {
            guard: GuardRef::Owned(self.guard()),
            set: &self,
        }
    }

    pub fn with_guard<'g>(&'g self, guard: &'g Guard) -> HashSetRef<'g, T, S> {
        HashSetRef {
            set: &self,
            guard: GuardRef::Ref(guard),
        }
    }
}

impl<T, S> HashSetRef<'_, T, S> {
    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn iter(&self) -> Keys<'_, T, ()> {
        self.set.iter(&self.guard)
    }
}

impl<T, S> HashSetRef<'_, T, S>
where
    T: Hash + Ord,
    S: BuildHasher,
{
    #[inline]
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.set.contains(value, &self.guard)
    }

    pub fn get<'g, Q>(&'g self, value: &Q) -> Option<&'g T>
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.set.get(value, &self.guard)
    }

    pub fn is_disjoint(&self, other: &HashSetRef<'_, T, S>) -> bool {
        self.set.is_disjoint(other.set, &self.guard, &other.guard)
    }

    pub fn is_subset(&self, other: &HashSetRef<'_, T, S>) -> bool {
        self.set.is_subset(other.set, &self.guard, &other.guard)
    }

    pub fn is_superset<'other>(&self, other: &HashSetRef<'other, T, S>) -> bool {
        self.set.is_superset(other.set, &self.guard, &other.guard)
    }
}

impl<T, S> HashSetRef<'_, T, S>
where
    T: 'static + Sync + Send + Clone + Hash + Ord,
    S: BuildHasher,
{
    pub fn insert(&self, value: T) -> bool {
        self.set.insert(value, &self.guard)
    }

    pub fn remove<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.set.remove(value, &self.guard)
    }

    pub fn take<'g, Q>(&'g self, value: &Q) -> Option<&'g T>
    where
        T: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.set.take(value, &self.guard)
    }

    pub fn retain<F>(&self, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.set.retain(f, &self.guard);
    }
}

impl<T, S> HashSetRef<'_, T, S>
where
    T: Clone + Ord,
{
    pub fn clear(&self) {
        self.set.clear(&self.guard);
    }

    pub fn reserve(&self, additional: usize) {
        self.set.reserve(additional, &self.guard)
    }
}

impl<'g, T, S> IntoIterator for &'g HashSetRef<'_, T, S> {
    type IntoIter = Keys<'g, T, ()>;
    type Item = &'g T;

    fn into_iter(self) -> Self::IntoIter {
        self.set.iter(&self.guard)
    }
}

impl<T, S> Debug for HashSetRef<'_, T, S>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self).finish()
    }
}

impl<T, S> Clone for HashSetRef<'_, T, S> {
    fn clone(&self) -> Self {
        self.set.pin()
    }
}

impl<T, S> PartialEq for HashSetRef<'_, T, S>
where
    T: Hash + Ord,
    S: BuildHasher,
{
    fn eq(&self, other: &Self) -> bool {
        self.set == other.set
    }
}

impl<T, S> PartialEq<HashSet<T, S>> for HashSetRef<'_, T, S>
where
    T: Hash + Ord,
    S: BuildHasher,
{
    fn eq(&self, other: &HashSet<T, S>) -> bool {
        self.set.guarded_eq(&other, &self.guard, &other.guard())
    }
}

impl<T, S> PartialEq<HashSetRef<'_, T, S>> for HashSet<T, S>
where
    T: Hash + Ord,
    S: BuildHasher,
{
    fn eq(&self, other: &HashSetRef<'_, T, S>) -> bool {
        self.guarded_eq(&other.set, &self.guard(), &other.guard)
    }
}

impl<T, S> Eq for HashSetRef<'_, T, S>
where
    T: Hash + Ord,
    S: BuildHasher,
{
}
