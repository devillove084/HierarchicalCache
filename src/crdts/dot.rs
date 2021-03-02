use std::cmp::{Ordering, PartialOrd};
use std::fmt;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use quickcheck::{Arbitrary, Gen};

/// Dot is a version marker for a single actor
#[derive(Clone, Serialize, Deserialize)]
pub struct Dot<A> {
    /// The actor identifier
    pub actor: A,
    /// The current version of this actor
    pub counter: u64,
}

impl<A> Dot<A> {
    /// Build a Dot from an actor and counter
    pub fn new(actor: A, counter: u64) -> Self {
        Self { actor, counter }
    }

    /// Increment this dot's counter
    pub fn apply_inc(&mut self) {
        self.counter += 1;
    }
}

impl<A: Clone> Dot<A> {
    /// Generate the successor of this dot
    pub fn inc(&self) -> Self {
        Self {
            actor: self.actor.clone(),
            counter: self.counter + 1,
        }
    }
}
impl<A: Copy> Copy for Dot<A> {}

impl<A: PartialEq> PartialEq for Dot<A> {
    fn eq(&self, other: &Self) -> bool {
        self.actor == other.actor && self.counter == other.counter
    }
}

impl<A: Eq> Eq for Dot<A> {}

impl<A: Hash> Hash for Dot<A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.actor.hash(state);
        self.counter.hash(state);
    }
}

impl<A: PartialOrd> PartialOrd for Dot<A> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.actor == other.actor {
            self.counter.partial_cmp(&other.counter)
        } else {
            None
        }
    }
}

impl<A: fmt::Debug> fmt::Debug for Dot<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}.{:?}", self.actor, self.counter)
    }
}

impl<A> From<(A, u64)> for Dot<A> {
    fn from(dot_material: (A, u64)) -> Self {
        let (actor, counter) = dot_material;
        Self { actor, counter }
    }
}

impl<A: Arbitrary + Clone> Arbitrary for Dot<A> {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        Dot {
            actor: A::arbitrary(g),
            counter: u64::arbitrary(g) % 50,
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let mut shrunk_dots = Vec::new();
        if self.counter > 0 {
            shrunk_dots.push(Self::new(self.actor.clone(), self.counter - 1));
        }
        Box::new(shrunk_dots.into_iter())
    }
}

/// An ordered dot.
/// dot's are first ordered by actor, dots from the same actor are ordered by counter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OrdDot<A: Ord> {
    /// The actor who created this dot.
    pub actor: A,
    /// The current counter of this actor.
    pub counter: u64,
}

impl<A: Ord> From<OrdDot<A>> for Dot<A> {
    fn from(OrdDot { actor, counter }: OrdDot<A>) -> Self {
        Self { actor, counter }
    }
}

impl<A: Ord> From<Dot<A>> for OrdDot<A> {
    fn from(Dot { actor, counter }: Dot<A>) -> Self {
        Self { actor, counter }
    }
}

/// A type for modeling a range of Dot's from one actor.
#[derive(Debug, PartialEq, Eq)]
pub struct DotRange<A> {
    /// The actor identifier
    pub actor: A,
    pub counter_range: core::ops::Range<u64>,
}

impl<A: fmt::Debug> fmt::Display for DotRange<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}.({}..{})",
            self.actor, self.counter_range.start, self.counter_range.end
        )
    }
}

impl<A: fmt::Debug> std::error::Error for DotRange<A> {}