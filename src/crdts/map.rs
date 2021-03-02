use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Debug, Display};
use std::hash::Hash;
use std::mem;

use serde::{Deserialize, Serialize};

use super::ctx::{AddCtx, ReadCtx, RmCtx};
//use crate::{CmRDT, CvRDT, Dot, ResetRemove, VClock};
use super::traits::{CmRDT, CvRDT};
use super::dot::{Dot};
use super::vclock::VClock;
use super::traits::ResetRemove;

/// Val Trait alias to reduce redundancy in type decl.
pub trait Val<A: Ord>: Clone + Default + ResetRemove<A> + CmRDT {}

impl<A, T> Val<A> for T
where
    A: Ord,
    T: Clone + Default + ResetRemove<A> + CmRDT,
{
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map<K: Ord, V: Val<A>, A: Ord + Hash> {
    // This clock stores the current version of the Map, it should
    // be greator or equal to all Entry.clock's in the Map.
    clock: VClock<A>,
    entries: BTreeMap<K, Entry<V, A>>,
    deferred: HashMap<VClock<A>, BTreeSet<K>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry<V: Val<A>, A: Ord> {
    // The entry clock tells us which actors edited this entry.
    clock: VClock<A>,

    // The nested CRDT
    val: V,
}

/// Operations which can be applied to the Map CRDT
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op<K: Ord, V: Val<A>, A: Ord> {
    /// Remove a key from the map
    Rm {
        /// The clock under which we will perform this remove
        clock: VClock<A>,
        /// Key to remove
        keyset: BTreeSet<K>,
    },
    /// Update an entry in the map
    Up {
        /// Actors version at the time of the update
        dot: Dot<A>,
        /// Key of the value to update
        key: K,
        /// The operation to apply on the value under `key`
        op: V::Op,
    },
}

impl<V: Val<A>, A: Ord> Default for Entry<V, A> {
    fn default() -> Self {
        Self {
            clock: VClock::default(),
            val: V::default(),
        }
    }
}

impl<K: Ord, V: Val<A>, A: Ord + Hash> Default for Map<K, V, A> {
    fn default() -> Self {
        Self {
            clock: Default::default(),
            entries: Default::default(),
            deferred: Default::default(),
        }
    }
}

impl<K: Ord, V: Val<A>, A: Ord + Hash> ResetRemove<A> for Map<K, V, A> {
    fn reset_remove(&mut self, clock: &VClock<A>) {
        self.entries = mem::take(&mut self.entries)
            .into_iter()
            .filter_map(|(key, mut entry)| {
                entry.clock.reset_remove(&clock);
                entry.val.reset_remove(&clock);
                if entry.clock.is_empty() {
                    None // remove this entry since its been forgotten
                } else {
                    Some((key, entry))
                }
            })
            .collect();

        self.deferred = mem::take(&mut self.deferred)
            .into_iter()
            .filter_map(|(mut rm_clock, key)| {
                rm_clock.reset_remove(&clock);
                if rm_clock.is_empty() {
                    None // this deferred remove has been forgotten
                } else {
                    Some((rm_clock, key))
                }
            })
            .collect();

        self.clock.reset_remove(&clock);
    }
}

/// The various validation errors that may occur when using a Map CRDT.
#[derive(Debug, PartialEq, Eq)]
pub enum CmRDTValidation<V: CmRDT, A> {
    /// We are missing dots specified in the DotRange
    SourceOrder(super::dot::DotRange<A>),

    /// There is a validation error in the nested CRDT.
    Value(V::Validation),
}

impl<V: CmRDT + Debug, A: Debug> Display for CmRDTValidation<V, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self, f)
    }
}

impl<V: CmRDT + Debug, A: Debug> std::error::Error for CmRDTValidation<V, A> {}

/// The various validation errors that may occur when using a Map CRDT.
#[derive(Debug, PartialEq, Eq)]
pub enum CvRDTValidation<K, V: CvRDT, A> {
    /// We've detected that two different members were inserted with the same dot.
    /// This can break associativity.
    DoubleSpentDot {
        /// The dot that was double spent
        dot: Dot<A>,
        /// Our member inserted with this dot
        our_key: K,
        /// Their member inserted with this dot
        their_key: K,
    },

    /// There is a validation error in the nested CRDT.
    Value(V::Validation),
}

impl<K: Debug, V: CvRDT + Debug, A: Debug> Display for CvRDTValidation<K, V, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self, f)
    }
}

impl<K: Debug, V: CvRDT + Debug, A: Debug> std::error::Error for CvRDTValidation<K, V, A> {}

impl<K: Ord, V: Val<A> + Debug, A: Ord + Hash + Clone + Debug> CmRDT for Map<K, V, A> {
    type Op = Op<K, V, A>;
    type Validation = CmRDTValidation<V, A>;

    fn validate_op(&self, op: &Self::Op) -> Result<(), Self::Validation> {
        match op {
            Op::Rm { .. } => Ok(()),
            Op::Up { dot, key, op } => {
                self.clock
                    .validate_op(&dot)
                    .map_err(CmRDTValidation::SourceOrder)?;
                let entry = self.entries.get(&key).cloned().unwrap_or_default();
                entry
                    .clock
                    .validate_op(&dot)
                    .map_err(CmRDTValidation::SourceOrder)?;
                entry.val.validate_op(&op).map_err(CmRDTValidation::Value)
            }
        }
    }

    fn apply(&mut self, op: Self::Op) {
        match op {
            Op::Rm { clock, keyset } => self.apply_keyset_rm(keyset, clock),
            Op::Up { dot, key, op } => {
                if self.clock.get(&dot.actor) >= dot.counter {
                    // we've seen this op already
                    return;
                }

                let entry = self.entries.entry(key).or_default();

                entry.clock.apply(dot.clone());
                entry.val.apply(op);

                self.clock.apply(dot);
                self.apply_deferred();
            }
        }
    }
}

impl<K: Ord + Clone + Debug, V: Val<A> + CvRDT + Debug, A: Ord + Hash + Clone + Debug> CvRDT
    for Map<K, V, A>
{
    type Validation = CvRDTValidation<K, V, A>;

    fn validate_merge(&self, other: &Self) -> Result<(), Self::Validation> {
        for (key, entry) in self.entries.iter() {
            for (other_key, other_entry) in other.entries.iter() {
                for Dot { actor, counter } in entry.clock.iter() {
                    if other_key != key && other_entry.clock.get(&actor) == counter {
                        return Err(CvRDTValidation::DoubleSpentDot {
                            dot: Dot::new(actor.clone(), counter),
                            our_key: key.clone(),
                            their_key: other_key.clone(),
                        });
                    }
                }

                if key == other_key && entry.clock.concurrent(&other_entry.clock) {
                    entry
                        .val
                        .validate_merge(&other_entry.val)
                        .map_err(CvRDTValidation::Value)?;
                }
            }
        }

        Ok(())
    }

    fn merge(&mut self, other: Self) {
        self.entries = mem::take(&mut self.entries)
            .into_iter()
            .filter_map(|(key, mut entry)| {
                if !other.entries.contains_key(&key) {
                    // other doesn't contain this entry because it:
                    //  1. has seen it and dropped it
                    //  2. hasn't seen it
                    if other.clock >= entry.clock {
                        // other has seen this entry and dropped it
                        None
                    } else {
                        // the other map has not seen this version of this
                        // entry, so add it. But first, we have to remove any
                        // information that may have been known at some point
                        // by the other map about this key and was removed.
                        entry.clock.reset_remove(&other.clock);
                        let mut removed_information = other.clock.clone();
                        removed_information.reset_remove(&entry.clock);
                        entry.val.reset_remove(&removed_information);
                        Some((key, entry))
                    }
                } else {
                    Some((key, entry))
                }
            })
            .collect();

        for (key, mut entry) in other.entries {
            if let Some(our_entry) = self.entries.get_mut(&key) {
                // SUBTLE: this entry is present in both maps, BUT that doesn't mean we
                // shouldn't drop it!
                // Perfectly possible that an item in both sets should be dropped
                let mut common = VClock::intersection(&entry.clock, &our_entry.clock);
                common.merge(entry.clock.clone_without(&self.clock));
                common.merge(our_entry.clock.clone_without(&other.clock));
                if common.is_empty() {
                    // both maps had seen each others entry and removed them
                    self.entries.remove(&key).unwrap();
                } else {
                    // we should not drop, as there is information still tracked in
                    // the common clock.
                    our_entry.val.merge(entry.val);

                    let mut information_that_was_deleted = entry.clock.clone();
                    information_that_was_deleted.merge(our_entry.clock.clone());
                    information_that_was_deleted.reset_remove(&common);
                    our_entry.val.reset_remove(&information_that_was_deleted);
                    our_entry.clock = common;
                }
            } else {
                // we don't have this entry, is it because we:
                //  1. have seen it and dropped it
                //  2. have not seen it
                if self.clock >= entry.clock {
                    // We've seen this entry and dropped it, we won't add it back
                } else {
                    // We have not seen this version of this entry, so we add it.
                    // but first, we have to remove the information on this entry
                    // that we have seen and deleted
                    entry.clock.reset_remove(&self.clock);

                    let mut information_we_deleted = self.clock.clone();
                    information_we_deleted.reset_remove(&entry.clock);
                    entry.val.reset_remove(&information_we_deleted);
                    self.entries.insert(key, entry);
                }
            }
        }

        // merge deferred removals
        for (rm_clock, keys) in other.deferred {
            self.apply_keyset_rm(keys, rm_clock);
        }

        self.clock.merge(other.clock);

        self.apply_deferred();
    }
}

impl<K: Ord, V: Val<A>, A: Ord + Hash + Clone> Map<K, V, A> {
    /// Constructs an empty Map
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns true if the map has no entries, false otherwise
    pub fn is_empty(&self) -> ReadCtx<bool, A> {
        ReadCtx {
            add_clock: self.clock.clone(),
            rm_clock: self.clock.clone(),
            val: self.entries.is_empty(),
        }
    }

    /// Returns the number of entries in the Map
    pub fn len(&self) -> ReadCtx<usize, A> {
        ReadCtx {
            add_clock: self.clock.clone(),
            rm_clock: self.clock.clone(),
            val: self.entries.len(),
        }
    }

    /// Retrieve value stored under a key
    pub fn get(&self, key: &K) -> ReadCtx<Option<V>, A> {
        let add_clock = self.clock.clone();
        let entry_opt = self.entries.get(&key);
        ReadCtx {
            add_clock,
            rm_clock: entry_opt
                .map(|map_entry| map_entry.clock.clone())
                .unwrap_or_default(),
            val: entry_opt.map(|map_entry| map_entry.val.clone()),
        }
    }

    /// Update a value under some key.
    ///
    /// If the key is not present in the map, the updater will be given the
    /// result of `V::default()`. The `default` value is used to ensure
    /// eventual consistency since our `Map`'s values are CRDTs themselves.
    ///
    /// The `impl Into<K>` bound provides a nice way of providing an input key that
    /// can easily convert to the `Map`'s key. For example, we can call this function
    /// with `"hello": &str` and it can be converted to `String`.
    pub fn update<F>(&self, key: impl Into<K>, ctx: AddCtx<A>, f: F) -> Op<K, V, A>
    where
        F: FnOnce(&V, AddCtx<A>) -> V::Op,
    {
        let key = key.into();
        let dot = ctx.dot.clone();
        let op = match self.entries.get(&key).map(|e| &e.val) {
            Some(data) => f(&data, ctx),
            None => f(&V::default(), ctx),
        };

        Op::Up { dot, key, op }
    }

    /// Remove an entry from the Map
    ///
    /// The `impl Into<K>` bound provides a nice way of providing an input key that
    /// can easily convert to the `Map`'s key. For example, we can call this function
    /// with `"hello": &str` and it can be converted to `String`.
    pub fn rm(&self, key: impl Into<K>, ctx: RmCtx<A>) -> Op<K, V, A> {
        let mut keyset = BTreeSet::new();
        keyset.insert(key.into());
        Op::Rm {
            clock: ctx.clock,
            keyset,
        }
    }

    /// Retrieve the current read context
    pub fn read_ctx(&self) -> ReadCtx<(), A> {
        ReadCtx {
            add_clock: self.clock.clone(),
            rm_clock: self.clock.clone(),
            val: (),
        }
    }

    /// apply the pending deferred removes
    fn apply_deferred(&mut self) {
        let deferred = mem::take(&mut self.deferred);
        for (clock, keys) in deferred {
            self.apply_keyset_rm(keys, clock);
        }
    }

    /// Apply a set of key removals given a clock.
    fn apply_keyset_rm(&mut self, mut keyset: BTreeSet<K>, clock: VClock<A>) {
        for key in keyset.iter() {
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.clock.reset_remove(&clock);
                if entry.clock.is_empty() {
                    // The entry clock says we have no info on this entry.
                    // So remove the entry
                    self.entries.remove(&key);
                } else {
                    // The entry clock is not empty so this means we still
                    // have some information on this entry, keep it.
                    entry.val.reset_remove(&clock);
                }
            }
        }

        // now we need to decide wether we should be keeping this
        // remove Op around to remove entries we haven't seen yet.
        match self.clock.partial_cmp(&clock) {
            None | Some(Ordering::Less) => {
                // this remove clock has information we don't have,
                // we need to log this in our deferred remove map, so
                // that we can delete keys that we haven't seen yet but
                // have been seen by this clock
                let deferred_set = self.deferred.entry(clock).or_default();
                deferred_set.append(&mut keyset);
            }
            _ => { /* we've seen all keys this clock has seen */ }
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = ReadCtx<&K, A>> {
        self.entries.iter().map(move |(k, v)| ReadCtx {
            add_clock: self.clock.clone(),
            rm_clock: v.clock.clone(),
            val: k,
        })
    }

    pub fn values(&self) -> impl Iterator<Item = ReadCtx<&V, A>> {
        self.entries.values().map(move |v| ReadCtx {
            add_clock: self.clock.clone(),
            rm_clock: v.clock.clone(),
            val: &v.val,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = ReadCtx<(&K, &V), A>> {
        self.entries.iter().map(move |(k, v)| ReadCtx {
            add_clock: self.clock.clone(),
            rm_clock: v.clock.clone(),
            val: (k, &v.val),
        })
    }
}