#![allow(missing_docs)]
#![allow(clippy::cognitive_complexity)]
use crossbeam_epoch::Guard;
use std::ops::Deref;

mod map;
mod map_ref;
mod node;
mod raw;
mod set;
mod set_ref;

#[cfg(feature = "rayon")]
mod rayon_impls;

#[cfg(feature = "serde")]
mod serde_impls;

/// Iterator types.
pub mod iter;

pub use map::{HashMap, TryInsertError};
pub use map_ref::HashMapRef;
pub use set::HashSet;
pub use set_ref::HashSetRef;

/// Default hasher for [`HashMap`].
pub type DefaultHashBuilder = ahash::RandomState;

/// Types needed to safely access shared data concurrently.
pub mod epoch {
    pub use crossbeam_epoch::{pin, Guard};
}

pub(crate) enum GuardRef<'g> {
    Owned(Guard),
    Ref(&'g Guard),
}

impl Deref for GuardRef<'_> {
    type Target = Guard;

    #[inline]
    fn deref(&self) -> &Guard {
        match *self {
            GuardRef::Owned(ref guard) | GuardRef::Ref(&ref guard) => guard,
        }
    }
}