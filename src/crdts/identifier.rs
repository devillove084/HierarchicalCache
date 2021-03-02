use core::fmt;

use num::{BigRational, One, Zero};
use quickcheck::{Arbitrary, Gen};
use serde::{Deserialize, Serialize};

fn rational_between(low: Option<&BigRational>, high: Option<&BigRational>) -> BigRational {
    match (low, high) {
        (None, None) => BigRational::zero(),
        (Some(low), None) => low + BigRational::one(),
        (None, Some(high)) => high - BigRational::one(),
        (Some(low), Some(high)) => (low + high) / BigRational::from_integer(2.into()),
    }
}

/// A dense Identifier, if you have two identifiers that are different, we can
/// always construct an identifier between them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier<T>(Vec<(BigRational, T)>);

impl<T> From<(BigRational, T)> for Identifier<T> {
    fn from((rational, value): (BigRational, T)) -> Self {
        Self(vec![(rational, value)])
    }
}

impl<T: Clone + Eq> Identifier<T> {
    /// Get a reference to the value this entry represents.
    pub fn value(&self) -> &T {
        self.0.last().map(|(_, elem)| elem).unwrap() // TODO: remove this unwrap
    }

    /// Get the value this entry represents, consuming the entry.
    pub fn into_value(mut self) -> T {
        self.0.pop().map(|(_, elem)| elem).unwrap() // TODO: remove this unwrap
    }

    /// Construct an entry between low and high holding the given element.
    pub fn between(low: Option<&Self>, high: Option<&Self>, elem: T) -> Self {
        match (low, high) {
            (Some(low), Some(high)) => {
                // Walk both paths until we reach a fork, constructing the path between these
                // two entries as we go.

                let mut path: Vec<(BigRational, T)> = vec![];
                let low_path = low.0.iter().cloned();
                let high_path = high.0.iter();
                let mut lower_bound = None;
                let mut upper_bound = None;
                for (l, h) in low_path.zip(high_path) {
                    if l.0 == h.0 {
                        // The entry between low and high will share the common path between these two
                        // entries. We accumulate this common prefix path as we traverse.
                        path.push(l)
                    } else {
                        // We find a spot where the lower and upper paths fork.
                        // We can insert our elem between these two bounds.
                        lower_bound = Some(l.0);
                        upper_bound = Some(&h.0);
                        break;
                    }
                }
                path.push((rational_between(lower_bound.as_ref(), upper_bound), elem));
                Self(path)
            }

            (low, high) => Self(vec![(
                rational_between(
                    low.and_then(|low_entry| low_entry.0.first().map(|(r, _)| r)),
                    high.and_then(|high_entry| high_entry.0.first().map(|(r, _)| r)),
                ),
                elem,
            )]),
        }
    }
}

impl<T: fmt::Display> fmt::Display for Identifier<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ID[")?;
        let mut iter = self.0.iter();
        if let Some((r, e)) = iter.next() {
            write!(f, "{}:{}", r, e)?;
        }
        for (r, e) in iter {
            write!(f, ", {}:{}", r, e)?;
        }
        write!(f, "]")
    }
}

impl<T: Arbitrary> Arbitrary for Identifier<T> {
    fn arbitrary<G: Gen>(g: &mut G) -> Self {
        let mut path = vec![];
        for _ in 0..(u8::arbitrary(g) % 7) {
            let ordering_index_material: Vec<(i64, i64)> = Arbitrary::arbitrary(g);
            let ordering_index = ordering_index_material
                .into_iter()
                .filter(|(_, d)| d != &0)
                .take(3)
                .map(|(n, d)| BigRational::new(n.into(), d.into()))
                .sum();
            path.push((ordering_index, T::arbitrary(g)));
        }
        Self(path)
    }
}