use std::{error, fmt};

use serde::{Deserialize, Serialize};

use super::traits::{CmRDT, CvRDT};

/// `LWWReg` is a simple CRDT that contains an arbitrary value
/// along with an `Ord` that tracks causality. It is the responsibility
/// of the user to guarantee that the source of the causal element
/// is monotonic. Don't use timestamps unless you are comfortable
/// with divergence.
///
/// `M` is a marker. It must grow monotonically *and* must be globally unique
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LWWReg<V, M> {
    /// `val` is the opaque element contained within this CRDT
    pub val: V,
    /// `marker` should be a monotonic value associated with this val
    pub marker: M,
}

impl<V: Default, M: Default> Default for LWWReg<V, M> {
    fn default() -> Self {
        Self {
            val: V::default(),
            marker: M::default(),
        }
    }
}

/// The Type of validation errors that may occur for an LWWReg.
#[derive(Debug, PartialEq)]
pub enum Validation {
    /// A conflicting change to a CRDT is witnessed by a dot that already exists.
    ConflictingMarker,
}

impl error::Error for Validation {
    fn description(&self) -> &str {
        match self {
            Validation::ConflictingMarker => {
                "A marker must be used exactly once, re-using the same marker breaks associativity"
            }
        }
    }
}

impl fmt::Display for Validation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl<V: PartialEq, M: Ord> CvRDT for LWWReg<V, M> {
    type Validation = Validation;

    /// Validates whether a merge is safe to perfom
    ///
    /// Returns an error if the marker is identical but the
    /// contained element is different.
    /// ```
    /// use crdts::{lwwreg, LWWReg, CvRDT};
    /// let mut l1 = LWWReg { val: 1, marker: 2 };
    /// let l2 = LWWReg { val: 3, marker: 2 };
    /// // errors!
    /// assert_eq!(l1.validate_merge(&l2), Err(lwwreg::Validation::ConflictingMarker));
    /// ```
    fn validate_merge(&self, other: &Self) -> Result<(), Self::Validation> {
        self.validate_update(&other.val, &other.marker)
    }

    /// Combines two `LWWReg` instances according to the marker that
    /// tracks causality.
    fn merge(&mut self, LWWReg { val, marker }: Self) {
        self.update(val, marker)
    }
}

impl<V: PartialEq, M: Ord> CmRDT for LWWReg<V, M> {
    // LWWReg's are small enough that we can replicate
    // the entire state as an Op
    type Op = Self;
    type Validation = Validation;

    fn validate_op(&self, op: &Self::Op) -> Result<(), Self::Validation> {
        self.validate_update(&op.val, &op.marker)
    }

    fn apply(&mut self, op: Self::Op) {
        self.merge(op)
    }
}

impl<V: PartialEq, M: Ord> LWWReg<V, M> {
    pub fn update(&mut self, val: V, marker: M) {
        if self.marker < marker {
            self.val = val;
            self.marker = marker;
        }
    }

    pub fn validate_update(&self, val: &V, marker: &M) -> Result<(), Validation> {
        if &self.marker == marker && val != &self.val {
            Err(Validation::ConflictingMarker)
        } else {
            Ok(())
        }
    }
}