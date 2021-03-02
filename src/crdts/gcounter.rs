use core::convert::Infallible;
use core::fmt::Debug;

use num::bigint::BigUint;
use serde::{Deserialize, Serialize};

//use crate::{CmRDT, CvRDT, Dot, ResetRemove, VClock};
use super::traits::{CmRDT, CvRDT, ResetRemove};
use super::dot::Dot;
use super::vclock::VClock;

#[derive(Debug, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub struct GCounter<A: Ord> {
    inner: VClock<A>,
}

impl<A: Ord> Default for GCounter<A> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

impl<A: Ord + Clone + Debug> CmRDT for GCounter<A> {
    type Op = Dot<A>;
    type Validation = Infallible;

    fn validate_op(&self, _op: &Self::Op) -> Result<(), Self::Validation> {
        Ok(())
    }

    fn apply(&mut self, op: Self::Op) {
        self.inner.apply(op)
    }
}

impl<A: Ord + Clone + Debug> CvRDT for GCounter<A> {
    type Validation = Infallible;

    fn validate_merge(&self, _other: &Self) -> Result<(), Self::Validation> {
        Ok(())
    }

    fn merge(&mut self, other: Self) {
        self.inner.merge(other.inner);
    }
}

impl<A: Ord> ResetRemove<A> for GCounter<A> {
    fn reset_remove(&mut self, clock: &VClock<A>) {
        self.inner.reset_remove(&clock);
    }
}

impl<A: Ord + Clone> GCounter<A> {
    /// Produce a new `GCounter`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Generate Op to increment the counter.
    pub fn inc(&self, actor: A) -> Dot<A> {
        self.inner.inc(actor)
    }

    /// Generate Op to increment the counter by a number of steps.
    pub fn inc_many(&self, actor: A, steps: u64) -> Dot<A> {
        let steps = steps + self.inner.get(&actor);
        Dot::new(actor, steps)
    }

    /// Return the current sum of this counter.
    pub fn read(&self) -> BigUint {
        self.inner.iter().map(|dot| dot.counter).sum()
    }
}