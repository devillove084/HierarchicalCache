use num::bigint::BigInt;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use super::traits::{CmRDT, CvRDT, ResetRemove};
//use crate::{Dot, GCounter, VClock};
use super::dot::Dot;
use super::vclock::VClock;
use super::gcounter::GCounter;

#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub struct PNCounter<A: Ord> {
    p: GCounter<A>,
    n: GCounter<A>,
}

/// The Direction of an Op.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Dir {
    /// signals that the op increments the counter
    Pos,
    /// signals that the op decrements the counter
    Neg,
}

/// An Op which is produced through from mutating the counter
/// Ship these ops to other replicas to have them sync up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Op<A: Ord> {
    /// The witnessing dot for this op
    pub dot: Dot<A>,
    /// the direction to move the counter
    pub dir: Dir,
}

impl<A: Ord> Default for PNCounter<A> {
    fn default() -> Self {
        Self {
            p: Default::default(),
            n: Default::default(),
        }
    }
}

impl<A: Ord + Clone + Debug> CmRDT for PNCounter<A> {
    type Op = Op<A>;
    type Validation = <GCounter<A> as CmRDT>::Validation;

    fn validate_op(&self, op: &Self::Op) -> Result<(), Self::Validation> {
        match op {
            Op { dot, dir: Dir::Pos } => self.p.validate_op(dot),
            Op { dot, dir: Dir::Neg } => self.n.validate_op(dot),
        }
    }

    fn apply(&mut self, op: Self::Op) {
        match op {
            Op { dot, dir: Dir::Pos } => self.p.apply(dot),
            Op { dot, dir: Dir::Neg } => self.n.apply(dot),
        }
    }
}

impl<A: Ord + Clone + Debug> CvRDT for PNCounter<A> {
    type Validation = <GCounter<A> as CvRDT>::Validation;

    fn validate_merge(&self, other: &Self) -> Result<(), Self::Validation> {
        self.p.validate_merge(&other.p)?;
        self.n.validate_merge(&other.n)
    }

    fn merge(&mut self, other: Self) {
        self.p.merge(other.p);
        self.n.merge(other.n);
    }
}

impl<A: Ord> ResetRemove<A> for PNCounter<A> {
    fn reset_remove(&mut self, clock: &VClock<A>) {
        self.p.reset_remove(&clock);
        self.n.reset_remove(&clock);
    }
}

impl<A: Ord + Clone> PNCounter<A> {
    /// Produce a new `PNCounter`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Generate an Op to increment the counter.
    pub fn inc(&self, actor: A) -> Op<A> {
        Op {
            dot: self.p.inc(actor),
            dir: Dir::Pos,
        }
    }

    /// Generate an Op to increment the counter.
    pub fn dec(&self, actor: A) -> Op<A> {
        Op {
            dot: self.n.inc(actor),
            dir: Dir::Neg,
        }
    }

    /// Generate an Op to increment the counter by a number of steps.
    pub fn inc_many(&self, actor: A, steps: u64) -> Op<A> {
        Op {
            dot: self.p.inc_many(actor, steps),
            dir: Dir::Pos,
        }
    }

    /// Generate an Op to decrement the counter by a number of steps.
    pub fn dec_many(&self, actor: A, steps: u64) -> Op<A> {
        Op {
            dot: self.n.inc_many(actor, steps),
            dir: Dir::Neg,
        }
    }

    /// Return the current value of this counter (P-N).
    pub fn read(&self) -> BigInt {
        let p: BigInt = self.p.read().into();
        let n: BigInt = self.n.read().into();
        p - n
    }
}