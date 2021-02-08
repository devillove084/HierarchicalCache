#![allow(unused_must_use)]
#![allow(dead_code)]
use std::{
    cmp, cmp::Ordering, default, fmt, hash, hash::Hash, iter, marker::PhantomData, mem, ops,
    ops::Bound,
};
use rand::prelude::*;
//use serde::{Serialize, Deserialize};

pub trait LevelGenerator {
    fn total(&self) -> usize;
    fn random(&mut self) -> usize;
}

pub struct GeometricalLevelGenerator {
    total: usize,
    p: f64,
    rng: rand::rngs::StdRng,
}

impl GeometricalLevelGenerator {
    pub fn new(total: usize, p: f64) -> Self {
        if total == 0 {
            panic!("total must be non-zero.");
        }
        if p <= 0.0 || p >= 1.0 {
            panic!("p must be in (0, 1).");
        }
        GeometricalLevelGenerator {
            total,
            p,
            rng: rand::rngs::StdRng::from_rng(thread_rng()).unwrap(),
        }
    }
}

impl LevelGenerator for GeometricalLevelGenerator {
    fn random(&mut self) -> usize {
        let mut h = 0;
        let mut x = self.p;
        let f = 1.0 - self.rng.gen::<f64>();
        while x > f && h + 1 < self.total {
            h += 1;
            x *= self.p
        }
        h
    }

    fn total(&self) -> usize {
        self.total
    }
}

pub struct SkipNode<V> {
    pub value: Option<V>,
    pub level: usize,
    pub next: Option<Box<SkipNode<V>>>,
    pub prev: Option<*mut SkipNode<V>>,
    pub links: Vec<Option<*mut SkipNode<V>>>,
    pub links_len: Vec<usize>,
}

impl<V> SkipNode<V> {
    pub fn head(total_levels: usize) -> Self {
        SkipNode {
            value: None,
            level: total_levels - 1,
            next: None,
            prev: None,
            links: iter::repeat(None).take(total_levels).collect(),
            links_len: iter::repeat(0).take(total_levels).collect(),
        }
    }

    pub fn new(value: V, level: usize) -> Self {
        SkipNode {
            value: Some(value),
            level,
            next: None,
            prev: None,
            links: iter::repeat(None).take(level + 1).collect(),
            links_len: iter::repeat(0).take(level + 1).collect(),
        }
    }

    pub fn into_inner(self) -> Option<V> {
        if self.value.is_some() {
            Some(self.value.unwrap())
        } else {
            None
        }
    }

    pub fn is_head(&self) -> bool {
        self.prev.is_none()
    }
}

impl<V> fmt::Display for SkipNode<V>
where
    V: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref v) = self.value {
            write!(f, "{}", v)
        } else {
            Ok(())
        }
    }
}


pub struct OrderedSkipList<T> {
    head: Box<SkipNode<T>>,
    len: usize,
    level_generator: GeometricalLevelGenerator,
    compare: Box<dyn Fn(&T, &T) -> Ordering>,
}

impl<T> OrderedSkipList<T>
where
    T: cmp::PartialOrd,
{
    #[inline]
    pub fn new() -> Self {
        let lg = GeometricalLevelGenerator::new(16, 1.0 / 2.0);
        OrderedSkipList {
            head: Box::new(SkipNode::head(lg.total())),
            len: 0,
            level_generator: lg,
            compare: (Box::new(|a: &T, b: &T| {
                a.partial_cmp(b).expect("Element cannot be ordered.")
            })) as Box<dyn Fn(&T, &T) -> Ordering>,
        }
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        let levels = cmp::max(1, (capacity as f64).log2().floor() as usize);
        let lg = GeometricalLevelGenerator::new(levels, 1.0 / 2.0);
        OrderedSkipList {
            head: Box::new(SkipNode::head(lg.total())),
            len: 0,
            level_generator: lg,
            compare: (Box::new(|a: &T, b: &T| {
                a.partial_cmp(b).expect("Element cannot be ordered.")
            })) as Box<dyn Fn(&T, &T) -> Ordering>,
        }
    }
}

impl<T> OrderedSkipList<T> {
    #[inline]
    pub unsafe fn with_comp<F>(f: F) -> Self
    where
        F: 'static + Fn(&T, &T) -> Ordering,
    {
        let lg = GeometricalLevelGenerator::new(16, 1.0 / 2.0);
        OrderedSkipList {
            head: Box::new(SkipNode::head(lg.total())),
            len: 0,
            level_generator: lg,
            compare: Box::new(f),
        }
    }

    pub unsafe fn sort_by<F>(&mut self, f: F)
    where
        F: 'static + Fn(&T, &T) -> Ordering,
    {
        let mut node: *mut SkipNode<T> = mem::transmute_copy(&self.head);

        while let Some(next) = (*node).links[0] {
            if let (&Some(ref a), &Some(ref b)) = (&(*node).value, &(*next).value) {
                if f(a, b) == Ordering::Greater {
                    panic!("New ordering function cannot be used.");
                }
            }
            node = next;
        }

        self.compare = Box::new(f);
    }

    #[inline]
    pub fn clear(&mut self) {
        unsafe {
            let node: *mut SkipNode<T> = mem::transmute_copy(&self.head);

            while let Some(ref mut next) = (*node).next {
                mem::replace(&mut (*node).next, mem::replace(&mut next.next, None));
            }
        }
        let new_head = Box::new(SkipNode::head(self.level_generator.total()));
        self.len = 0;
        mem::replace(&mut self.head, new_head);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn insert(&mut self, value: T) {
        unsafe {
            self.len += 1;

            let mut new_node = Box::new(SkipNode::new(value, self.level_generator.random()));
            let new_node_ptr: *mut SkipNode<T> = mem::transmute_copy(&new_node);

            let mut insert_node: *mut SkipNode<T> = mem::transmute_copy(&self.head);
            let mut insert_nodes: Vec<*mut SkipNode<T>> = Vec::with_capacity(new_node.level);

            let mut lvl = self.level_generator.total();
            while lvl > 0 {
                lvl -= 1;

                while let Some(next) = (*insert_node).links[lvl] {
                    if let (&Some(ref a), &Some(ref b)) = (&(*next).value, &new_node.value) {
                        if (self.compare)(a, b) == Ordering::Less {
                            insert_node = next;
                            continue;
                        }
                    }
                    break;
                }
                if lvl <= new_node.level {
                    insert_nodes.push(insert_node);
                    new_node.links[lvl] = (*insert_node).links[lvl];
                    (*insert_node).links[lvl] = Some(new_node_ptr);
                } else {
                    (*insert_node).links_len[lvl] += 1;
                }
            }

            for (lvl, &insert_node) in insert_nodes.iter().rev().enumerate() {
                if lvl == 0 {
                    (*insert_node).links_len[lvl] = if (*insert_node).is_head() { 0 } else { 1 };
                    new_node.links_len[lvl] = 1;
                } else {
                    let length = self
                        .link_length(insert_node, Some(new_node_ptr), lvl)
                        .unwrap();
                    new_node.links_len[lvl] = (*insert_node).links_len[lvl] - length + 1;
                    (*insert_node).links_len[lvl] = length;
                }
            }

            new_node.prev = Some(insert_node);
            if let Some(next) = (*new_node).links[0] {
                (*next).prev = Some(new_node_ptr);
            }

            let tmp = mem::replace(&mut (*insert_node).next, Some(new_node));
            if let Some(ref mut node) = (*insert_node).next {
                node.next = tmp;
            }
        }
    }

    #[inline]
    pub fn front(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(&self[0])
        }
    }

    #[inline]
    pub fn back(&self) -> Option<&T> {
        let len = self.len();
        if len > 0 {
            Some(&self[len - 1])
        } else {
            None
        }
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<&T> {
        let len = self.len();
        if index < len {
            Some(&self[index])
        } else {
            None
        }
    }

    #[inline]
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            Some(self.remove_index(0))
        }
    }

    #[inline]
    pub fn pop_back(&mut self) -> Option<T> {
        let len = self.len();
        if len > 0 {
            Some(self.remove_index(len - 1))
        } else {
            None
        }
    }

    pub fn contains(&self, value: &T) -> bool {
        unsafe {
            let mut node: *const SkipNode<T> = mem::transmute_copy(&self.head);

            let mut lvl = self.level_generator.total();
            while lvl > 0 {
                lvl -= 1;

                while let Some(next) = (*node).links[lvl] {
                    if let Some(ref next_value) = (*next).value {
                        match (self.compare)(next_value, value) {
                            Ordering::Less => {
                                node = next;
                                continue;
                            }
                            Ordering::Equal => {
                                return true;
                            }
                            Ordering::Greater => {
                                break;
                            }
                        }
                    }
                }
            }

            false
        }
    }

    pub fn remove(&mut self, value: &T) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        unsafe {
            let mut node: *mut SkipNode<T> = mem::transmute_copy(&self.head);
            let mut return_node: Option<*mut SkipNode<T>> = None;
            let mut prev_nodes: Vec<*mut SkipNode<T>> =
                Vec::with_capacity(self.level_generator.total());

            let mut lvl = self.level_generator.total();
            while lvl > 0 {
                lvl -= 1;

                if let Some(return_node) = return_node {
                    while let Some(next) = (*node).links[lvl] {
                        if next == return_node {
                            prev_nodes.push(node);
                            break;
                        } else {
                            node = next;
                        }
                    }
                } else {
                    while let Some(next) = (*node).links[lvl] {
                        if let Some(ref next_value) = (*next).value {
                            match (self.compare)(next_value, value) {
                                Ordering::Less => {
                                    node = next;
                                    continue;
                                }
                                Ordering::Equal => {
                                    return_node = Some(next);
                                    prev_nodes.push(node);
                                    break;
                                }
                                Ordering::Greater => {
                                    prev_nodes.push(node);
                                    break;
                                }
                            }
                        }
                    }
                    if (*node).links[lvl].is_none() {
                        prev_nodes.push(node);
                        continue;
                    }
                }
            }

            if let Some(return_node) = return_node {
                for (lvl, &prev_node) in prev_nodes.iter().rev().enumerate() {
                    if (*prev_node).links[lvl] == Some(return_node) {
                        (*prev_node).links[lvl] = (*return_node).links[lvl];
                        (*prev_node).links_len[lvl] += (*return_node).links_len[lvl] - 1;
                    } else {
                        (*prev_node).links_len[lvl] -= 1;
                    }
                }
                if let Some(next_node) = (*return_node).links[0] {
                    (*next_node).prev = (*return_node).prev;
                }
                self.len -= 1;
                mem::replace(
                    &mut (*(*return_node).prev.unwrap()).next,
                    mem::replace(&mut (*return_node).next, None),
                )
                .unwrap()
                .into_inner()
            } else {
                None
            }
        }
    }

    pub fn remove_first(&mut self, value: &T) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        unsafe {
            let mut node: *mut SkipNode<T> = mem::transmute_copy(&self.head);
            let mut return_node: Option<*mut SkipNode<T>> = None;
            let mut prev_nodes: Vec<*mut SkipNode<T>> =
                Vec::with_capacity(self.level_generator.total());

            let mut lvl = self.level_generator.total();
            while lvl > 0 {
                lvl -= 1;

                if let Some(return_node) = return_node {
                    while let Some(next) = (*node).links[lvl] {
                        if next == return_node {
                            prev_nodes.push(node);
                            break;
                        } else {
                            node = next;
                        }
                    }
                } else {
                    while let Some(next) = (*node).links[lvl] {
                        if let Some(ref next_value) = (*next).value {
                            match (self.compare)(next_value, value) {
                                Ordering::Less => {
                                    node = next;
                                    continue;
                                }
                                Ordering::Equal => {
                                    if let Some(ref prev_value) = (*(*next).prev.unwrap()).value {
                                        if (self.compare)(prev_value, next_value) == Ordering::Equal
                                        {
                                            prev_nodes.push(node);
                                            break;
                                        }
                                    }
                                    return_node = Some(next);
                                    prev_nodes.push(node);
                                    break;
                                }
                                Ordering::Greater => {
                                    prev_nodes.push(node);
                                    break;
                                }
                            }
                        }
                    }
                    if (*node).links[lvl].is_none() {
                        prev_nodes.push(node);
                        continue;
                    }
                }
            }

            if let Some(return_node) = return_node {
                for (lvl, &prev_node) in prev_nodes.iter().rev().enumerate() {
                    if (*prev_node).links[lvl] == Some(return_node) {
                        (*prev_node).links[lvl] = (*return_node).links[lvl];
                        (*prev_node).links_len[lvl] += (*return_node).links_len[lvl] - 1;
                    } else {
                        (*prev_node).links_len[lvl] -= 1;
                    }
                }
                if let Some(next_node) = (*return_node).links[0] {
                    (*next_node).prev = (*return_node).prev;
                }
                self.len -= 1;
                mem::replace(
                    &mut (*(*return_node).prev.unwrap()).next,
                    mem::replace(&mut (*return_node).next, None),
                )
                .expect("Popped node shouldn't be None.")
                .into_inner()
            } else {
                None
            }
        }
    }

    pub fn remove_index(&mut self, index: usize) -> T {
        unsafe {
            if index >= self.len() {
                panic!("Index out of bounds.");
            } else {
                let mut node: *mut SkipNode<T> = mem::transmute_copy(&self.head);
                let mut return_node: *mut SkipNode<T> = mem::transmute_copy(&self.head);
                let mut index_sum = 0;
                let mut lvl = self.level_generator.total();
                while lvl > 0 {
                    lvl -= 1;
                    while index_sum + (*node).links_len[lvl] < index {
                        index_sum += (*node).links_len[lvl];
                        node = (*node).links[lvl].unwrap();
                    }
                    if index_sum + (*node).links_len[lvl] == index {
                        if let Some(next) = (*node).links[lvl] {
                            return_node = next;
                            (*node).links[lvl] = (*next).links[lvl];
                            (*node).links_len[lvl] += (*next).links_len[lvl] - 1;
                        }
                    } else {
                        (*node).links_len[lvl] -= 1;
                    }
                }

                if let Some(next) = (*return_node).links[0] {
                    (*next).prev = (*return_node).prev;
                }
                self.len -= 1;
                mem::replace(
                    &mut (*(*return_node).prev.unwrap()).next,
                    mem::replace(&mut (*return_node).next, None),
                )
                .unwrap()
                .into_inner()
                .unwrap()
            }
        }
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        unsafe {
            let mut removed_nodes = Vec::new();

            for lvl in 0..self.level_generator.total() {
                let mut node: *mut SkipNode<T> = mem::transmute_copy(&self.head);
                loop {
                    if let Some(next) = (*node).links[lvl] {
                        if let Some(ref value) = (*next).value {
                            if !f(value) {
                                (*node).links[lvl] = (*next).links[lvl];
                                if lvl == 0 {
                                    removed_nodes.push(next);
                                }
                                continue;
                            }
                        }
                    }
                    (*node).links_len[lvl] =
                        self.link_length(node, (*node).links[lvl], lvl).unwrap();
                    if let Some(next) = (*node).links[lvl] {
                        node = next;
                    } else {
                        break;
                    }
                }
            }

            self.len -= removed_nodes.len();
            for node in removed_nodes {
                if let Some(next) = (*node).links[0] {
                    (*next).prev = (*node).prev;
                }
                if let Some(prev) = (*node).prev {
                    mem::replace(&mut (*prev).next, mem::replace(&mut (*node).next, None));
                }
            }
        }
    }

    pub fn dedup(&mut self) {
        unsafe {
            let mut removed_nodes = Vec::new();

            for lvl in 0..self.level_generator.total() {
                let mut node: *mut SkipNode<T> = mem::transmute_copy(&self.head);
                loop {
                    if let Some(next) = (*node).links[lvl] {
                        if lvl == 0 {
                            if let (&Some(ref a), &Some(ref b)) = (&(*node).value, &(*next).value) {
                                if (self.compare)(a, b) == Ordering::Equal {
                                    (*node).links[lvl] = (*next).links[lvl];
                                    removed_nodes.push(next);
                                    continue;
                                }
                            }
                        } else {
                            let mut next_is_removed = false;
                            for &removed in &removed_nodes {
                                if next == removed {
                                    next_is_removed = true;
                                    break;
                                }
                            }
                            if next_is_removed {
                                (*node).links[lvl] = (*next).links[lvl];
                                continue;
                            }
                        }
                    }
                    (*node).links_len[lvl] =
                        self.link_length(node, (*node).links[lvl], lvl).unwrap();
                    if let Some(next) = (*node).links[lvl] {
                        node = next;
                    } else {
                        break;
                    }
                }
            }

            self.len -= removed_nodes.len();
            for node in removed_nodes {
                if let Some(next) = (*node).links[0] {
                    (*next).prev = (*node).prev;
                }
                if let Some(prev) = (*node).prev {
                    mem::replace(&mut (*prev).next, mem::replace(&mut (*node).next, None));
                }
            }
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn into_iter(self) -> IntoIter<T> {
        IntoIter {
            head: unsafe { mem::transmute_copy(&self.head) },
            end: self.get_last() as *mut SkipNode<T>,
            size: self.len(),
            skiplist: self,
        }
    }

    pub fn iter(&self) -> Iter<T> {
        Iter {
            start: unsafe { mem::transmute_copy(&self.head) },
            end: self.get_last(),
            size: self.len(),
            _lifetime: PhantomData,
        }
    }

    pub fn range(&self, min: Bound<&T>, max: Bound<&T>) -> Iter<T> {
        unsafe {
            let start = match min {
                Bound::Included(min) => {
                    let mut node = self.find_value(min);
                    if let Some(ref value) = (*node).value {
                        if (self.compare)(value, min) == Ordering::Equal {
                            while let Some(prev) = (*node).prev {
                                if let Some(ref value) = (*prev).value {
                                    if (self.compare)(value, min) == Ordering::Equal {
                                        node = prev;
                                        continue;
                                    }
                                }
                                break;
                            }
                            node = (*node).prev.unwrap();
                        }
                    }
                    node
                }
                Bound::Excluded(min) => {
                    let mut node = self.find_value(min);
                    while let Some(next) = (*node).links[0] {
                        if let Some(ref value) = (*next).value {
                            if (self.compare)(value, min) == Ordering::Equal {
                                node = next;
                                continue;
                            }
                        }
                        break;
                    }
                    node
                }
                Bound::Unbounded => mem::transmute_copy(&self.head),
            };
            let end = match max {
                Bound::Included(max) => {
                    let mut node = self.find_value(max);
                    if let Some(ref value) = (*node).value {
                        if (self.compare)(value, max) == Ordering::Equal {
                            while let Some(next) = (*node).links[0] {
                                if let Some(ref value) = (*next).value {
                                    if (self.compare)(value, max) == Ordering::Equal {
                                        node = next;
                                        continue;
                                    }
                                }
                                break;
                            }
                        }
                    }
                    node
                }
                Bound::Excluded(max) => {
                    let mut node = self.find_value(max);
                    if let Some(ref value) = (*node).value {
                        if (self.compare)(value, max) == Ordering::Equal {
                            while let Some(prev) = (*node).prev {
                                if let Some(ref value) = (*prev).value {
                                    if (self.compare)(value, max) == Ordering::Equal {
                                        node = prev;
                                        continue;
                                    }
                                }
                                break;
                            }
                            node = (*node).prev.unwrap();
                        }
                    }
                    node
                }
                Bound::Unbounded => self.get_last(),
            };
            match self.link_length(
                start as *mut SkipNode<T>,
                Some(end as *mut SkipNode<T>),
                cmp::min((*start).level, (*end).level) + 1,
            ) {
                Ok(l) => Iter {
                    start,
                    end,
                    size: l,
                    _lifetime: PhantomData,
                },
                Err(()) => Iter {
                    start,
                    end: start,
                    size: 0,
                    _lifetime: PhantomData,
                },
            }
        }
    }
}

impl<T> OrderedSkipList<T> {
    #[allow(dead_code)]
    fn check(&self) {
        unsafe {
            let mut node: *const SkipNode<T> = mem::transmute_copy(&self.head);
            assert!((*node).is_head() && (*node).value.is_none() && (*node).prev.is_none());

            let mut length_sum;
            for lvl in 0..self.level_generator.total() {
                length_sum = 0;
                node = mem::transmute_copy(&self.head);

                loop {
                    length_sum += (*node).links_len[lvl];
                    assert_eq!((*node).level + 1, (*node).links.len());
                    assert_eq!((*node).level + 1, (*node).links_len.len());
                    assert_eq!(
                        (*node).links_len[lvl],
                        self.link_length(node as *mut SkipNode<T>, (*node).links[lvl], lvl)
                            .unwrap()
                    );

                    if lvl == 0 {
                        assert!((*node).next.is_some() == (*node).links[lvl].is_some());

                        if let Some(prev) = (*node).prev {
                            assert_eq!((*prev).links[lvl], Some(node as *mut SkipNode<T>));
                            assert_eq!(node, mem::transmute_copy((*prev).next.as_ref().unwrap()));
                        }
                    }

                    if let Some(next) = (*node).links[lvl] {
                        assert!((*next).value.is_some());
                        node = next;
                    } else {
                        break;
                    }
                }
                assert_eq!(length_sum, self.len());
            }
        }
    }

    fn link_length(
        &self,
        start: *mut SkipNode<T>,
        end: Option<*mut SkipNode<T>>,
        lvl: usize,
    ) -> Result<usize, ()> {
        unsafe {
            let mut length = 0;
            let mut node = start;
            if lvl == 0 {
                while Some(node) != end {
                    length += 1;
                    if (*node).is_head() {
                        length -= 1;
                    }
                    match (*node).links[lvl] {
                        Some(ptr) => node = ptr,
                        None => break,
                    }
                }
            } else {
                while Some(node) != end {
                    length += (*node).links_len[lvl - 1];
                    match (*node).links[lvl - 1] {
                        Some(ptr) => node = ptr,
                        None => break,
                    }
                }
            }
            if let Some(end) = end {
                if node != end {
                    return Err(());
                }
            }
            Ok(length)
        }
    }

    fn get_last(&self) -> *const SkipNode<T> {
        unsafe {
            let mut node: *const SkipNode<T> = mem::transmute_copy(&self.head);

            let mut lvl = self.level_generator.total();
            while lvl > 0 {
                lvl -= 1;

                while let Some(next) = (*node).links[lvl] {
                    node = next;
                }
            }
            node
        }
    }

    fn find_value(&self, value: &T) -> *const SkipNode<T> {
        unsafe {
            let mut node: *const SkipNode<T> = mem::transmute_copy(&self.head);

            let mut lvl = self.level_generator.total();
            while lvl > 0 {
                lvl -= 1;

                while let Some(next) = (*node).links[lvl] {
                    if let Some(ref next_value) = (*next).value {
                        match (self.compare)(next_value, value) {
                            Ordering::Less => node = next,
                            Ordering::Equal => {
                                node = next;
                                return node;
                            }
                            Ordering::Greater => break,
                        }
                    } else {
                        panic!("Encountered a value-less node.");
                    }
                }
            }

            node
        }
    }

    fn get_index(&self, index: usize) -> *const SkipNode<T> {
        unsafe {
            if index >= self.len() {
                panic!("Index out of bounds.");
            } else {
                let mut node: *const SkipNode<T> = mem::transmute_copy(&self.head);

                let mut index_sum = 0;
                let mut lvl = self.level_generator.total();
                while lvl > 0 {
                    lvl -= 1;

                    while index_sum + (*node).links_len[lvl] <= index {
                        index_sum += (*node).links_len[lvl];
                        node = (*node).links[lvl].unwrap();
                    }
                }
                node
            }
        }
    }
}

impl<T> OrderedSkipList<T>
where
    T: fmt::Debug,
{
    #[allow(dead_code)]
    fn debug_structure(&self) {
        unsafe {
            let mut node: *const SkipNode<T> = mem::transmute_copy(&self.head);
            let mut rows: Vec<_> = iter::repeat(String::new())
                .take(self.level_generator.total())
                .collect();

            loop {
                let value = if let Some(ref v) = (*node).value {
                    format!("> [{:?}]", v)
                } else {
                    "> []".to_string()
                };

                let max_str_len = format!("{} -{}-", value, (*node).links_len[(*node).level]).len();

                let mut lvl = self.level_generator.total();
                while lvl > 0 {
                    lvl -= 1;

                    let mut value_len = if lvl <= (*node).level {
                        format!("{} -{}-", value, (*node).links_len[lvl])
                    } else {
                        format!("{} -", value)
                    };
                    for _ in 0..(max_str_len - value_len.len()) {
                        value_len.push('-');
                    }

                    let mut dashes = String::new();
                    for _ in 0..value_len.len() {
                        dashes.push('-');
                    }

                    if lvl <= (*node).level {
                        rows[lvl].push_str(value_len.as_ref());
                    } else {
                        rows[lvl].push_str(dashes.as_ref());
                    }
                }

                if let Some(next) = (*node).links[0] {
                    node = next;
                } else {
                    break;
                }
            }

            for row in rows.iter().rev() {
                println!("{}", row);
            }
        }
    }
}

unsafe impl<T: Send> Send for OrderedSkipList<T> {}
unsafe impl<T: Sync> Sync for OrderedSkipList<T> {}

impl<T> ops::Drop for OrderedSkipList<T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            let node: *mut SkipNode<T> = mem::transmute_copy(&self.head);

            while let Some(ref mut next) = (*node).next {
                mem::replace(&mut (*node).next, mem::replace(&mut next.next, None));
            }
        }
    }
}

impl<T: PartialOrd> default::Default for OrderedSkipList<T> {
    fn default() -> OrderedSkipList<T> {
        OrderedSkipList::new()
    }
}

impl<A, B> cmp::PartialEq<OrderedSkipList<B>> for OrderedSkipList<A>
where
    A: cmp::PartialEq<B>,
{
    #[inline]
    fn eq(&self, other: &OrderedSkipList<B>) -> bool {
        self.len() == other.len() && self.iter().eq(other)
    }
    #[allow(clippy::partialeq_ne_impl)]
    #[inline]
    fn ne(&self, other: &OrderedSkipList<B>) -> bool {
        self.len != other.len || self.iter().ne(other)
    }
}

impl<T> cmp::Eq for OrderedSkipList<T> where T: cmp::Eq {}

impl<A, B> cmp::PartialOrd<OrderedSkipList<B>> for OrderedSkipList<A>
where
    A: cmp::PartialOrd<B>,
{
    #[inline]
    fn partial_cmp(&self, other: &OrderedSkipList<B>) -> Option<Ordering> {
        self.iter().partial_cmp(other)
    }
}

impl<T> Ord for OrderedSkipList<T>
where
    T: cmp::Ord,
{
    #[inline]
    fn cmp(&self, other: &OrderedSkipList<T>) -> Ordering {
        self.iter().cmp(other)
    }
}

impl<T> Extend<T> for OrderedSkipList<T> {
    #[inline]
    fn extend<I: iter::IntoIterator<Item = T>>(&mut self, iterable: I) {
        let iterator = iterable.into_iter();
        for element in iterator {
            self.insert(element);
        }
    }
}

impl<T> ops::Index<usize> for OrderedSkipList<T> {
    type Output = T;

    fn index(&self, index: usize) -> &T {
        unsafe { (*self.get_index(index)).value.as_ref().unwrap() }
    }
}

impl<T> fmt::Debug for OrderedSkipList<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;

        for (i, entry) in self.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{:?}", entry)?;
        }
        write!(f, "]")
    }
}

impl<T> fmt::Display for OrderedSkipList<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;

        for (i, entry) in self.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", entry)?;
        }
        write!(f, "]")
    }
}

impl<T> iter::IntoIterator for OrderedSkipList<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> IntoIter<T> {
        self.into_iter()
    }
}
impl<'a, T> iter::IntoIterator for &'a OrderedSkipList<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}
impl<'a, T> iter::IntoIterator for &'a mut OrderedSkipList<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Iter<'a, T> {
        self.iter()
    }
}

impl<T> iter::FromIterator<T> for OrderedSkipList<T>
where
    T: PartialOrd,
{
    #[inline]
    fn from_iter<I>(iter: I) -> OrderedSkipList<T>
    where
        I: iter::IntoIterator<Item = T>,
    {
        let mut skiplist = OrderedSkipList::new();
        skiplist.extend(iter);
        skiplist
    }
}

impl<T: Hash> Hash for OrderedSkipList<T> {
    #[inline]
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        for elt in self {
            elt.hash(state);
        }
    }
}

pub struct Iter<'a, T: 'a> {
    start: *const SkipNode<T>,
    end: *const SkipNode<T>,
    size: usize,
    _lifetime: PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        unsafe {
            if self.start == self.end {
                return None;
            }
            if let Some(next) = (*self.start).links[0] {
                self.start = next;
                if self.size > 0 {
                    self.size -= 1;
                }
                return (*self.start).value.as_ref();
            }
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size, Some(self.size))
    }
}

impl<'a, T> DoubleEndedIterator for Iter<'a, T> {
    fn next_back(&mut self) -> Option<&'a T> {
        unsafe {
            if self.end == self.start {
                return None;
            }
            if let Some(prev) = (*self.end).prev {
                let node = self.end;
                if prev as *const SkipNode<T> != self.start {
                    self.size -= 1;
                } else {
                    self.size = 0;
                }
                self.end = prev;
                return (*node).value.as_ref();
            }
            None
        }
    }
}

pub struct IntoIter<T> {
    skiplist: OrderedSkipList<T>,
    head: *mut SkipNode<T>,
    end: *mut SkipNode<T>,
    size: usize,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        unsafe {
            if let Some(next) = (*self.head).links[0] {
                for lvl in 0..self.skiplist.level_generator.total() {
                    if lvl <= (*next).level {
                        (*self.head).links[lvl] = (*next).links[lvl];
                        (*self.head).links_len[lvl] = (*next).links_len[lvl] - 1;
                    } else {
                        (*self.head).links_len[lvl] -= 1;
                    }
                }
                if let Some(next) = (*self.head).links[0] {
                    (*next).prev = Some(self.head);
                }
                self.skiplist.len -= 1;
                self.size -= 1;
                let popped_node = mem::replace(
                    &mut (*self.head).next,
                    mem::replace(&mut (*next).next, None),
                );
                popped_node.expect("Should have a node").value
            } else {
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size, Some(self.size))
    }
}

impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        unsafe {
            if self.head == self.end {
                return None;
            }
            if let Some(prev) = (*self.end).prev {
                if prev as *const SkipNode<T> != self.head {
                    self.size -= 1;
                } else {
                    self.size = 0;
                }
                self.end = prev;
                (*self.end).links[0] = None;
                let node = mem::replace(&mut (*self.end).next, None);
                return node.unwrap().into_inner();
            }
            None
        }
    }
}
