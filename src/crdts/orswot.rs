/// Observed-Remove Set With Out Tombstones (ORSWOT), ported directly from `riak_dt`.
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::mem;

use serde::{Deserialize, Serialize};

use super::ctx::{AddCtx, ReadCtx, RmCtx};
use quickcheck::{Arbitrary, Gen};
//use crate::{CmRDT, CvRDT, Dot, ResetRemove, VClock};
use super::traits::{CmRDT, CvRDT, ResetRemove};
use super::dot::Dot;
use super::vclock::VClock;

/// `Orswot` is an add-biased or-set without tombstones ported from
/// the riak_dt CRDT library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Orswot<M: Hash + Eq, A: Ord + Hash> {
    pub(crate) clock: VClock<A>,
    pub(crate) entries: HashMap<M, VClock<A>>,
    pub(crate) deferred: HashMap<VClock<A>, HashSet<M>>,
}

/// Op's define an edit to an Orswot, Op's must be replayed in the exact order
/// they were produced to guarantee convergence.
///
/// Op's are idempotent, that is, applying an Op twice will not have an effect
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Op<M, A: Ord> {
    /// Add members to the set
    Add {
        /// witnessing dot
        dot: Dot<A>,
        /// Members to add
        members: Vec<M>,
    },
    /// Remove member from the set
    Rm {
        /// witnessing clock
        clock: VClock<A>,
        /// Members to remove
        members: Vec<M>,
    },
}

impl<M: Hash + Eq, A: Ord + Hash> Default for Orswot<M, A> {
    fn default() -> Self {
        Orswot {
            clock: Default::default(),
            entries: Default::default(),
            deferred: Default::default(),
        }
    }
}

impl<M: Hash + Clone + Eq, A: Ord + Hash + Clone + Debug> CmRDT for Orswot<M, A> {
    type Op = Op<M, A>;
    type Validation = <VClock<A> as CmRDT>::Validation;

    fn validate_op(&self, op: &Self::Op) -> Result<(), Self::Validation> {
        match op {
            Op::Add { dot, .. } => self.clock.validate_op(dot),
            Op::Rm { .. } => Ok(()),
        }
    }

    fn apply(&mut self, op: Self::Op) {
        match op {
            Op::Add { dot, members } => {
                if self.clock.get(&dot.actor) >= dot.counter {
                    // we've already seen this op
                    return;
                }

                for member in members {
                    let member_vclock = self.entries.entry(member).or_default();
                    member_vclock.apply(dot.clone());
                }

                self.clock.apply(dot);
                self.apply_deferred();
            }
            Op::Rm { clock, members } => {
                self.apply_rm(members.into_iter().collect(), clock);
            }
        }
    }
}

/// The variations that an ORSWOT may fail validation.
#[derive(Debug, PartialEq, Eq)]
pub enum Validation<M, A> {
    /// We've detected that two different members were inserted with the same dot.
    /// This can break associativity.
    DoubleSpentDot {
        /// The dot that was double spent
        dot: Dot<A>,
        /// Our member inserted with this dot
        our_member: M,
        /// Their member inserted with this dot
        their_member: M,
    },
}

impl<M: Debug, A: Debug> Display for Validation<M, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self, f)
    }
}

impl<M: Debug, A: Debug> std::error::Error for Validation<M, A> {}

impl<M: Hash + Eq + Clone + Debug, A: Ord + Hash + Clone + Debug> CvRDT for Orswot<M, A> {
    type Validation = Validation<M, A>;

    fn validate_merge(&self, other: &Self) -> Result<(), Self::Validation> {
        for (member, clock) in self.entries.iter() {
            for (other_member, other_clock) in other.entries.iter() {
                for Dot { actor, counter } in clock.iter() {
                    if other_member != member && other_clock.get(&actor) == counter {
                        return Err(Validation::DoubleSpentDot {
                            dot: Dot::new(actor.clone(), counter),
                            our_member: member.clone(),
                            their_member: other_member.clone(),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Merge combines another `Orswot` with this one.
    fn merge(&mut self, other: Self) {
        self.entries = mem::take(&mut self.entries)
            .into_iter()
            .filter_map(|(entry, mut clock)| {
                if !other.entries.contains_key(&entry) {
                    // other doesn't contain this entry because it:
                    //  1. has seen it and dropped it
                    //  2. hasn't seen it
                    if other.clock >= clock {
                        // other has seen this entry and dropped it
                        None
                    } else {
                        // the other map has not seen this version of this
                        // entry, so add it. But first, we have to remove any
                        // information that may have been known at some point
                        // by the other map about this key and was removed.
                        clock.reset_remove(&other.clock);
                        Some((entry, clock))
                    }
                } else {
                    Some((entry, clock))
                }
            })
            .collect();

        for (entry, mut clock) in other.entries {
            if let Some(our_clock) = self.entries.get_mut(&entry) {
                // SUBTLE: this entry is present in both orswots, BUT that doesn't mean we
                // shouldn't drop it!
                // Perfectly possible that an item in both sets should be dropped
                let mut common = VClock::intersection(&clock, &our_clock);
                common.merge(clock.clone_without(&self.clock));
                common.merge(our_clock.clone_without(&other.clock));
                if common.is_empty() {
                    // both maps had seen each others entry and removed them
                    self.entries.remove(&entry).unwrap();
                } else {
                    // we should not drop, as there is information still tracked in
                    // the common clock.
                    *our_clock = common;
                }
            } else {
                // we don't have this entry, is it because we:
                //  1. have seen it and dropped it
                //  2. have not seen it
                if self.clock >= clock {
                    // We've seen this entry and dropped it, we won't add it back
                } else {
                    // We have not seen this version of this entry, so we add it.
                    // but first, we have to remove the information on this entry
                    // that we have seen and deleted
                    clock.reset_remove(&self.clock);
                    self.entries.insert(entry, clock);
                }
            }
        }

        // merge deferred removals
        for (rm_clock, members) in other.deferred {
            self.apply_rm(members, rm_clock);
        }

        self.clock.merge(other.clock);

        self.apply_deferred();
    }
}

impl<M: Hash + Clone + Eq, A: Ord + Hash> ResetRemove<A> for Orswot<M, A> {
    fn reset_remove(&mut self, clock: &VClock<A>) {
        self.clock.reset_remove(&clock);

        self.entries = mem::take(&mut self.entries)
            .into_iter()
            .filter_map(|(val, mut val_clock)| {
                val_clock.reset_remove(&clock);
                if val_clock.is_empty() {
                    None
                } else {
                    Some((val, val_clock))
                }
            })
            .collect();

        self.deferred = mem::take(&mut self.deferred)
            .into_iter()
            .filter_map(|(mut vclock, deferred)| {
                vclock.reset_remove(&clock);
                if vclock.is_empty() {
                    None
                } else {
                    Some((vclock, deferred))
                }
            })
            .collect();
    }
}

impl<M: Hash + Clone + Eq, A: Ord + Hash + Clone> Orswot<M, A> {
    /// Returns a new `Orswot` instance.
    pub fn new() -> Self {
        Default::default()
    }

    /// Return a snapshot of the ORSWOT clock
    pub fn clock(&self) -> VClock<A> {
        self.clock.clone()
    }

    /// Add a single element.
    pub fn add(&self, member: M, ctx: AddCtx<A>) -> Op<M, A> {
        Op::Add {
            dot: ctx.dot,
            members: std::iter::once(member).collect(),
        }
    }

    /// Add multiple elements.
    pub fn add_all<I: IntoIterator<Item = M>>(&self, members: I, ctx: AddCtx<A>) -> Op<M, A> {
        Op::Add {
            dot: ctx.dot,
            members: members.into_iter().collect(),
        }
    }

    /// Remove a member with a witnessing ctx.
    pub fn rm(&self, member: M, ctx: RmCtx<A>) -> Op<M, A> {
        Op::Rm {
            clock: ctx.clock,
            members: std::iter::once(member).collect(),
        }
    }

    /// Remove members with a witnessing ctx.
    pub fn rm_all<I: IntoIterator<Item = M>>(&self, members: I, ctx: RmCtx<A>) -> Op<M, A> {
        Op::Rm {
            clock: ctx.clock,
            members: members.into_iter().collect(),
        }
    }

    /// Remove members using a witnessing clock.
    fn apply_rm(&mut self, members: HashSet<M>, clock: VClock<A>) {
        for member in members.iter() {
            if let Some(member_clock) = self.entries.get_mut(&member) {
                member_clock.reset_remove(&clock);
                if member_clock.is_empty() {
                    self.entries.remove(&member);
                }
            }
        }

        match clock.partial_cmp(&self.clock) {
            None | Some(Ordering::Greater) => {
                if let Some(existing_deferred) = self.deferred.get_mut(&clock) {
                    existing_deferred.extend(members);
                } else {
                    self.deferred.insert(clock, members);
                }
            }
            _ => { /* we've already seen this remove */ }
        }
    }

    /// Check if the set contains a member
    pub fn contains(&self, member: &M) -> ReadCtx<bool, A> {
        let member_clock_opt = self.entries.get(&member);
        let exists = member_clock_opt.is_some();
        ReadCtx {
            add_clock: self.clock.clone(),
            rm_clock: member_clock_opt.cloned().unwrap_or_default(),
            val: exists,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = ReadCtx<&M, A>> {
        self.entries.iter().map(move |(m, clock)| ReadCtx {
            add_clock: self.clock.clone(),
            rm_clock: clock.clone(),
            val: m,
        })
    }

    /// Retrieve the current members.
    pub fn read(&self) -> ReadCtx<HashSet<M>, A> {
        ReadCtx {
            add_clock: self.clock.clone(),
            rm_clock: self.clock.clone(),
            val: self.entries.keys().cloned().collect(),
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

    fn apply_deferred(&mut self) {
        let deferred = mem::take(&mut self.deferred);
        for (clock, entries) in deferred.into_iter() {
            self.apply_rm(entries, clock)
        }
    }
}

impl<A: Ord + Hash + Arbitrary + Debug, M: Hash + Eq + Arbitrary> Arbitrary for Op<M, A> {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        let dot = Dot::arbitrary(g);
        let clock = VClock::arbitrary(g);

        let mut members_set = HashSet::new();
        for _ in 0..u8::arbitrary(g) % 10 {
            members_set.insert(M::arbitrary(g));
        }
        let members: Vec<_> = members_set.into_iter().collect();

        match u8::arbitrary(g) % 2 {
            0 => Op::Add { members, dot },
            1 => Op::Rm { members, clock },
            _ => panic!("tried to generate invalid op"),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let mut shrunk_ops = Vec::new();
        match self {
            Op::Add { members, dot } => {
                for (i, _m) in members.iter().enumerate() {
                    let mut shrunk_members = members.clone();
                    shrunk_members.remove(i);

                    shrunk_ops.push(Op::Add {
                        members: shrunk_members,
                        dot: dot.clone(),
                    });
                }

                dot.shrink().for_each(|shrunk_dot| {
                    shrunk_ops.push(Op::Add {
                        members: members.clone(),
                        dot: shrunk_dot,
                    })
                });
            }
            Op::Rm { members, clock } => {
                for (i, _m) in members.iter().enumerate() {
                    let mut shrunk_members = members.clone();
                    shrunk_members.remove(i);

                    shrunk_ops.push(Op::Rm {
                        members: shrunk_members,
                        clock: clock.clone(),
                    });
                }

                clock.shrink().for_each(|shrunk_clock| {
                    shrunk_ops.push(Op::Rm {
                        members: members.clone(),
                        clock: shrunk_clock,
                    })
                });
            }
        }

        Box::new(shrunk_ops.into_iter())
    }
}

impl<M: Debug, A: Ord + Hash + Debug> Debug for Op<M, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Op::Add { dot, members } => write!(f, "Add({:?}, {:?})", dot, members),
            Op::Rm { clock, members } => write!(f, "Rm({:?}, {:?})", clock, members),
        }
    }
}