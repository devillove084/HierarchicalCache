use crate::ops::RVec;
use crate::types::{Count, Index, Key, Score};
use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};

// TODO: Use convenient-skiplist

// TODO: Why doesn't this actually allow it?
#[allow(clippy::mutable_key_type)]
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct SortedSetMember {
    pub score: Score,
    pub member: String,
}

impl PartialOrd<SortedSetMember> for SortedSetMember {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let score_cmp = self.score.cmp(&other.score);
        if let Ordering::Equal = score_cmp {
            return Some(self.member.cmp(&other.member));
        }
        Some(score_cmp)
    }
}

impl Ord for SortedSetMember {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

// TODO: Look into using RangeBounds properly
// impl RangeBounds<Score> for SortedSetMember {
//     fn start_bound(&self) -> Bound<&Score> {
//         Included(&self.score)
//     }

//     fn end_bound(&self) -> Bound<&Score> {
//         Included(&self.score)
//     }
//     fn contains<U>(&self, item: &U) -> bool
//     where
//         U: PartialOrd<Score> + ?Sized,
//     {
//         if let Ordering::Equal = item.partial_cmp(&self.score).unwrap() {
//             true
//         } else {
//             false
//         }
//     }
// }

// impl RangeBounds<SortedSetMember> for Range<Score> {
//     fn start_bound(&self) -> Bound<&SortedSetMember> {
//         let f = SortedSetMember::new(&b"".to_vec(), self.start);
//         Included(f)
//     }
//     fn end_bound(&self) -> Bound<&SortedSetMember> {
//         let f = SortedSetMember::new(&b"".to_vec(), self.end);
//         Included(&f)
//     }

// fn contains<U>(&self, item: &U) -> bool
// where
//     U: PartialOrd<Score> + ?Sized {
//     if let Ordering::Equal = item.partial_cmp(&self.score).unwrap() {
//         true
//     } else {
//         false
//     }
// }
// }

impl SortedSetMember {
    fn new(key: &[u8], score: Score) -> Self {
        SortedSetMember {
            score,
            member: String::from_utf8_lossy(key).to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[allow(clippy::mutable_key_type)]
pub struct SortedSet {
    members_hash: HashMap<Key, Score>,
    scores: BTreeSet<SortedSetMember>,
}

#[allow(unused)]
impl SortedSet {
    /// Create a new SortedSet
    pub fn new() -> Self {
        SortedSet::default()
    }

    /// Add the following keys and scores to the sorted set
    pub fn add(&mut self, key_scores: RVec<(Score, Key)>) -> Count {
        key_scores
            .into_iter()
            .map(|(score, key)| match self.members_hash.entry(key) {
                Entry::Vacant(ent) => {
                    self.scores.insert(SortedSetMember::new(ent.key(), score));
                    ent.insert(score);
                    1
                }
                Entry::Occupied(_) => 0,
            })
            .sum()
    }

    /// Remove the following keys from the sorted set
    pub fn remove(&mut self, keys: &[Key]) -> Count {
        keys.iter()
            .map(|key| match self.members_hash.remove(key) {
                None => 0,
                Some(score) => {
                    let tmp = SortedSetMember::new(key, score);
                    self.scores.remove(&tmp);
                    1
                }
            })
            .sum()
    }

    fn remove_one(&mut self, key: &Key) {
        self.members_hash.remove(key);
    }

    /// Returns the number of members stored in the set.
    pub fn card(&self) -> Count {
        self.members_hash.len() as Count
    }

    /// Return the score of the member in the sorted set
    pub fn score(&self, key: Key) -> Option<Score> {
        self.members_hash.get(&key).cloned()
    }

    /// Get all members between (lower, upper) scores
    pub fn range(&self, range: (Score, Score)) -> RVec<SortedSetMember> {
        // TODO: Use a more efficient method. I should use a skiplist or an AVL tree.
        // Another option is to retackle the rangebounds stuff, but the semantics are different.
        // I want to be able to compare by score AND member when inserting/removing,
        // but only by score in this case. Need to figure out how to encode that.
        self.scores
            .iter()
            .filter(|mem| range.0 <= mem.score && mem.score <= range.1)
            .cloned()
            .collect()
    }

    /// Remove count (default: 1) maximum members from the sorted set
    pub fn pop_max(&mut self, count: Count) -> Vec<SortedSetMember> {
        let count = count as usize; // TODO: What if it's negative?
        let ret: Vec<SortedSetMember> = self.scores.iter().rev().take(count).cloned().collect();
        for key in ret.iter().map(|s| s.member.clone()) {
            self.remove(&[key.into()]);
        }
        ret
    }

    /// Remove count (default: 1) minimum members from the sorted set
    pub fn pop_min(&mut self, count: Count) -> Vec<SortedSetMember> {
        let count = count as usize; // TODO: What if it's negative?
        let ret: Vec<SortedSetMember> = self.scores.iter().take(count).cloned().collect();
        for key in ret.iter().map(|s| s.member.clone()) {
            self.remove(&[key.into()]);
        }
        ret
    }

    // /// Get the maximum score in the sorted set
    // pub fn max_score(&self) -> Option<Score> {
    //     self.scores.iter().rev().next().cloned().map(|m| m.score)
    // }

    /// Get the rank of a given key in the sorted set
    pub fn rank(&self, key: Key) -> Option<Index> {
        self.scores
            .iter()
            .position(|s| s.member.as_bytes() == &*key)
            .map(|pos| pos as Index)
    }
}