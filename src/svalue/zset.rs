use super::object::RobjPtr;
use super::skip_list::SkipList;
use super::dict::Dict;
use super::hash;
use rand::prelude::*;

pub struct Zset {
    dict: Dict<RobjPtr, RobjPtr>,
    list: SkipList,
}

impl Zset {
    pub fn new() -> Zset {
        Zset {
            dict: Dict::new(hash::string_object_hash, rand::thread_rng().gen()),
            list: SkipList::new(),
        }
    }
}