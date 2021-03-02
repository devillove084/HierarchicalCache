use core::convert::Infallible;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::traits::{CmRDT, CvRDT};

/// A `GSet` is a grow-only set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GSet<T: Ord> {
    value: BTreeSet<T>,
}

impl<T: Ord> Default for GSet<T> {
    fn default() -> Self {
        GSet::new()
    }
}

impl<T: Ord> From<GSet<T>> for BTreeSet<T> {
    fn from(gset: GSet<T>) -> BTreeSet<T> {
        gset.value
    }
}

impl<T: Ord> CvRDT for GSet<T> {
    type Validation = Infallible;

    fn validate_merge(&self, _other: &Self) -> Result<(), Self::Validation> {
        Ok(())
    }

    fn merge(&mut self, other: Self) {
        other.value.into_iter().for_each(|e| self.insert(e))
    }
}

impl<T: Ord> CmRDT for GSet<T> {
    type Op = T;
    type Validation = Infallible;

    fn validate_op(&self, _op: &Self::Op) -> Result<(), Self::Validation> {
        Ok(())
    }

    fn apply(&mut self, op: Self::Op) {
        self.insert(op);
    }
}

impl<T: Ord> GSet<T> {
    /// Instantiates an empty `GSet`.
    pub fn new() -> Self {
        Self {
            value: BTreeSet::new(),
        }
    }

    pub fn insert(&mut self, element: T) {
        self.value.insert(element);
    }

    pub fn contains(&self, element: &T) -> bool {
        self.value.contains(element)
    }

    pub fn read(&self) -> BTreeSet<T>
    where
        T: Clone,
    {
        self.value.clone()
    }
}
