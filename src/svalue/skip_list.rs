use std::rc::{Rc, Weak};
use super::{RobjPtr, Robj};
use rand::prelude::*;
use std::cell::RefCell;
use std::iter::Iterator;

const SKIP_LIST_MAX_LEVEL: usize = 32;

pub struct SkipListLevel {
    forward: Option<Rc<RefCell<SkipListNode>>>,
    span: usize,
}

pub struct SkipListNode {
    obj: Option<RobjPtr>,
    score: f64,
    backward: Option<Weak<RefCell<SkipListNode>>>,
    level: Vec<SkipListLevel>,
}

impl SkipListNode {
    fn new(level: usize, score: f64, obj: Option<RobjPtr>) -> SkipListNode {
        let mut level_vec: Vec<SkipListLevel>
            = Vec::with_capacity(level);

        for _ in 0..level {
            level_vec.push(SkipListLevel {
                forward: None,
                span: 0,
            });
        }

        let mut node = SkipListNode {
            obj: None,
            score,
            backward: None,
            level: level_vec,
        };

        if let Some(p) = obj {
            node.obj = Some(p);
        }

        node
    }

    fn obj_ref(&self) -> &RobjPtr {
        self.obj.as_ref().unwrap()
    }

    fn iter(&self, level: usize) -> SkipListNextNodeIter {
        let forward = self.level[level].forward.as_ref();

        let mut i = SkipListNextNodeIter {
            next: None,
            level,
        };

        if let Some(n) = forward {
            i.next = Some(Rc::clone(n));
        }

        i
    }
}

pub struct SkipListNextNodeIter {
    next: Option<Rc<RefCell<SkipListNode>>>,
    level: usize,
}

impl Iterator for SkipListNextNodeIter {
    type Item = Rc<RefCell<SkipListNode>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let None = self.next {
            return None;
        }
        let ret = Rc::clone(&self.next.as_ref().unwrap());
        let this_node = ret
            .borrow();
        let forward = this_node
            .level[self.level]
            .forward
            .as_ref();

        match forward {
            None => self.next = None,
            Some(_) => self.next = Some(Rc::clone(forward.unwrap())),
        }

        Some(Rc::clone(&ret))
    }
}

pub struct SkipList {
    header: Rc<RefCell<SkipListNode>>,
    tail: Option<Rc<RefCell<SkipListNode>>>,
    length: usize,
    level: usize,
}

impl SkipList {
    pub fn new() -> SkipList {
        let mut header =
            SkipListNode::new(SKIP_LIST_MAX_LEVEL, 0.0, None);

        header.backward = None;

        SkipList {
            header: Rc::new(RefCell::new(header)),
            tail: None,
            length: 0,
            level: 1,
        }
    }

    fn random_level() -> usize {
        let mut level = 1usize;
        let mut rng = rand::thread_rng();
        let mut num: usize = rng.gen();

        while ((num & 0xFFFFusize) as f64) < (0.25 * (0xFFFF as f64)) {
            level += 1;
            num = rng.gen();
        }

        if level < SKIP_LIST_MAX_LEVEL {
            return level;
        }
        SKIP_LIST_MAX_LEVEL
    }

    pub fn insert(&mut self, score: f64, obj: RobjPtr) {
        let mut update: Vec<Option<Rc<RefCell<SkipListNode>>>> =
            Vec::with_capacity(SKIP_LIST_MAX_LEVEL);

        for _ in 0..SKIP_LIST_MAX_LEVEL {
            update.push(None)
        }

        let mut rank = [0usize; SKIP_LIST_MAX_LEVEL];

        let mut x = Rc::clone(&self.header);

        for i in (0..self.level).rev() {
            rank[i] = if i == self.level - 1 {
                0
            } else {
                rank[i + 1]
            };

            let mut span = x.borrow().level[i].span;
            for node in x.clone().borrow().iter(i) {
                let inner = node.borrow();
                let inner_obj = inner.obj_ref().borrow();
                if inner.score > score || (inner.score == score &&
                    inner_obj.string() >= obj.borrow().string()) {
                    break;
                }
                rank[i] += span;
                span = node.borrow().level[i].span;
                x = Rc::clone(&node);
            }

            update[i] = Some(Rc::clone(&x));
        }

        let level = SkipList::random_level();

        if level > self.level {
            for i in self.level..level {
                rank[i] = 0;
                update[i] = Some(Rc::clone(&self.header));
                update[i].as_ref().unwrap()
                    .as_ref().borrow_mut().level[i].span = self.length;
            }

            self.level = level;
        }

        let new_node = Rc::new(
            RefCell::new(
                SkipListNode::new(level, score, Some(obj))
            )
        );
        let curr = new_node.as_ref();

        for i in 0..level {
            let prev = update[i].as_ref().unwrap().as_ref();

            curr.borrow_mut().level[i].forward = match prev.borrow().level[i].forward {
                None => None,
                Some(_) => Some(Rc::clone(prev.borrow().level[i]
                    .forward.as_ref().unwrap())),
            };

            prev.borrow_mut().level[i].forward = Some(Rc::clone(&new_node));

            curr.borrow_mut().level[i].span
                = prev.borrow().level[i].span - (rank[0] - rank[i]);

            prev.borrow_mut().level[i].span = (rank[0] - rank[i]) + 1;
        }

        for i in level..self.level {
            update[i].as_ref().unwrap().as_ref().borrow_mut().level[i].span += 1;
        }

        curr.borrow_mut().backward = if Rc::ptr_eq(
            &self.header, update[0].as_ref().unwrap(),
        ) {
            None
        } else {
            Some(Rc::downgrade(update[0].as_ref().unwrap()))
        };

        if let Some(e) = curr.borrow().level[0].forward.as_ref() {
            e.as_ref().borrow_mut().backward = Some(Rc::downgrade(&new_node));
        } else {
            self.tail = Some(Rc::clone(&new_node));
        }

        self.length += 1;
    }

    pub fn first_in_range(&self, range: &RangeSpec) -> Option<Rc<RefCell<SkipListNode>>> {
        if !self.is_in_range(&range) {
            return None;
        }

        let mut x = Rc::clone(&self.header);

        for i in (0..self.level).rev() {
            for node in x.clone().borrow().iter(i) {
                let score = node.borrow().score;
                if RangeSpec::value_gte_min(score, &range) {
                    break;
                }
                x = Rc::clone(&node);
            }
        }

        x = Rc::clone(x.clone()
                          .as_ref()
                          .borrow()
                          .level[0]
                          .forward
                          .as_ref()
                          .unwrap() // this is an inner range, so the next cannot be None
        );

        let score = x.borrow().score;
        if !RangeSpec::value_lte_max(score, &range) {
            return None;
        }

        Some(x)
    }

    pub fn last_in_range(&self, range: &RangeSpec) -> Option<Rc<RefCell<SkipListNode>>> {
        if !self.is_in_range(&range) {
            return None;
        }

        let mut x = Rc::clone(&self.header);

        for i in (0..self.level).rev() {
            for node in x.clone().borrow().iter(i) {
                let score = node.borrow().score;
                if !RangeSpec::value_lte_max(score, &range) {
                    break;
                }
                x = Rc::clone(&node);
            }
        }

        let score = x.borrow().score;
        if !RangeSpec::value_gte_min(score, &range) {
            return None;
        }

        Some(x)
    }

    pub fn delete(&mut self, score: f64, obj: &RobjPtr) -> bool {
        let mut update: Vec<Option<Rc<RefCell<SkipListNode>>>> =
            (0..SKIP_LIST_MAX_LEVEL).map(|_| None).collect();

        let mut x = Rc::clone(&self.header);

        for i in (0..self.level).rev() {
            for node in x.clone().borrow().iter(i) {
                let inner = node.borrow();
                let inner_obj = inner.obj_ref().borrow();
                if inner.score > score || (inner.score == score &&
                    inner_obj.string() >= obj.borrow().string()) {
                    break;
                }

                x = Rc::clone(&node);
            }
            update[i] = Some(Rc::clone(&x));
        }

        let x = x.borrow().iter(0).next();
        if let Some(e) = x {
            if e.borrow().score == score
                && Robj::string_obj_eq(e.borrow().obj_ref(), obj) {
                self.delete_node(&e, &update);
                return true;
            }
        }
        false
    }

    fn delete_node(&mut self, node: &Rc<RefCell<SkipListNode>>,
                   update: &Vec<Option<Rc<RefCell<SkipListNode>>>>) {
        let this_node = node.borrow();
        for i in 0..self.level {
            let mut up = update[i].as_ref().unwrap().as_ref().borrow_mut();
            let up_next = up.level[i].forward.as_ref();
            if up_next.is_some() && Rc::ptr_eq(up_next.unwrap(), node) {
                let forward = this_node.level[i].forward.as_ref();
                up.level[i].span +=
                    this_node.level[i].span - 1;
                up.level[i].forward =
                    match forward {
                        None => None,
                        Some(r) => Some(Rc::clone(r)),
                    }
            } else {
                up.level[i].span -= 1;
            }
        }

        if let Some(e) = this_node.level[0].forward.as_ref() {
            e.as_ref().borrow_mut().backward = match this_node.backward {
                None => None,
                Some(ref k) => Some(Weak::clone(k)),
            }
        } else {
            self.tail = match this_node.backward {
                None => None,
                Some(ref k) => Some(k.upgrade().unwrap()),
            }
        }

        while self.level > 1 &&
            self.header.borrow().level[self.level - 1].forward.is_none() {
            self.level -= 1;
        }
        self.length -= 1;
    }


    pub fn is_in_range(&self, range: &RangeSpec) -> bool {
        if range.min > range.max ||
            (range.min == range.max && (range.minex || range.maxex)) {
            return false;
        }

        let highest = self.highest_score();
        if highest.is_none() ||
            !RangeSpec::value_gte_min(highest.unwrap(), &range) {
            return false;
        }

        let lowest = self.lowest_score();
        if lowest.is_none() ||
            !RangeSpec::value_lte_max(lowest.unwrap(), &range) {
            return false;
        }

        true
    }

    pub fn highest_score(&self) -> Option<f64> {
        match self.tail {
            None => None,
            Some(_) => Some(self.tail
                .as_ref()
                .unwrap()
                .as_ref()
                .borrow()
                .score
            ),
        }
    }

    pub fn lowest_score(&self) -> Option<f64> {
        match self.header.borrow().level[0].forward {
            None => None,
            Some(_) => Some(self.header
                .as_ref()
                .borrow()
                .level[0]
                .forward
                .as_ref()
                .unwrap()
                .as_ref()
                .borrow()
                .score
            )
        }
    }
}

pub struct RangeSpec {
    min: f64,
    max: f64,
    minex: bool,
    maxex: bool,
}

impl RangeSpec {
    fn new(min: f64, minex: bool, max: f64, maxex: bool) -> RangeSpec {
        RangeSpec {
            min,
            max,
            minex,
            maxex,
        }
    }

    fn new_closed(min: f64, max: f64) -> RangeSpec {
        RangeSpec {
            min,
            max,
            minex: false,
            maxex: false,
        }
    }

    fn new_open(min: f64, max: f64) -> RangeSpec {
        RangeSpec {
            min,
            max,
            minex: true,
            maxex: true,
        }
    }

    fn value_gte_min(value: f64, range: &Self) -> bool {
        match range.minex {
            true => value > range.min,
            false => value >= range.min,
        }
    }

    fn value_lte_max(value: f64, range: &Self) -> bool {
        match range.maxex {
            true => value < range.max,
            false => value <= range.max,
        }
    }
}