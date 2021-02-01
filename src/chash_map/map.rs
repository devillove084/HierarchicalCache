#![allow(deprecated)]
use crate::iter::*;
use crate::node::*;
use crate::raw::*;
use crossbeam_epoch::{self as epoch, Atomic, Guard, Owned, Shared};
use std::borrow::Borrow;
use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};
use std::hash::{BuildHasher, Hash, Hasher};
use std::iter::FromIterator;
use std::sync::atomic::{AtomicIsize, Ordering};


const ISIZE_BITS: usize = core::mem::size_of::<isize>() * 8;

const MAXIMUM_CAPACITY: usize = 1 << 30;

const DEFAULT_CAPACITY: usize = 16;

const TREEIFY_THRESHOLD: usize = 8;

const UNTREEIFY_THRESHOLD: usize = 6;

const MIN_TREEIFY_CAPACITY: usize = 64;

const MIN_TRANSFER_STRIDE: isize = 16;

const RESIZE_STAMP_BITS: usize = ISIZE_BITS / 2;

const MAX_RESIZERS: isize = (1 << (ISIZE_BITS - RESIZE_STAMP_BITS)) - 1;

const RESIZE_STAMP_SHIFT: usize = ISIZE_BITS - RESIZE_STAMP_BITS;

#[cfg(not(miri))]
static NCPU_INITIALIZER: std::sync::Once = std::sync::Once::new();
#[cfg(not(miri))]
static NCPU: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

macro_rules! load_factor {
    ($n: expr) => {
        $n - ($n >> 2)
    };
}

pub struct HashMap<K, V, S = crate::DefaultHashBuilder> {
    table: Atomic<Table<K, V>>,

    next_table: Atomic<Table<K, V>>,

    transfer_index: AtomicIsize,

    count: AtomicIsize,

    size_ctl: AtomicIsize,

    collector: epoch::Collector,

    build_hasher: S,
}

#[derive(Eq, PartialEq, Clone, Debug)]
enum PutResult<'a, T> {
    Inserted {
        new: &'a T,
    },
    Replaced {
        old: &'a T,
        new: &'a T,
    },
    Exists {
        current: &'a T,
        not_inserted: Box<T>,
    },
}

impl<'a, T> PutResult<'a, T> {
    fn before(&self) -> Option<&'a T> {
        match *self {
            PutResult::Inserted { .. } => None,
            PutResult::Replaced { old, .. } => Some(old),
            PutResult::Exists { current, .. } => Some(current),
        }
    }

    #[allow(dead_code)]
    fn after(&self) -> Option<&'a T> {
        match *self {
            PutResult::Inserted { new } => Some(new),
            PutResult::Replaced { new, .. } => Some(new),
            PutResult::Exists { .. } => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TryInsertError<'a, V> {
    pub current: &'a V,
    pub not_inserted: V,
}

impl<'a, V> Display for TryInsertError<'a, V>
where
    V: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Insert of \"{:?}\" failed as key was already present with value \"{:?}\"",
            self.not_inserted, self.current
        )
    }
}

impl<'a, V> Error for TryInsertError<'a, V>
where
    V: Debug,
{
    #[inline]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl<K, V> HashMap<K, V, crate::DefaultHashBuilder> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hasher(capacity, crate::DefaultHashBuilder::default())
    }
}

impl<K, V, S> Default for HashMap<K, V, S>
where
    S: Default,
{
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}

impl<K, V, S> HashMap<K, V, S> {
    pub fn with_hasher(hash_builder: S) -> Self {
        Self {
            table: Atomic::null(),
            next_table: Atomic::null(),
            transfer_index: AtomicIsize::new(0),
            count: AtomicIsize::new(0),
            size_ctl: AtomicIsize::new(0),
            build_hasher: hash_builder,
            collector: epoch::default_collector().clone(),
        }
    }

    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        if capacity == 0 {
            return Self::with_hasher(hash_builder);
        }

        let mut map = Self::with_hasher(hash_builder);
        map.presize(capacity);
        map
    }

    pub fn guard(&self) -> epoch::Guard {
        self.collector.register().pin()
    }

    #[inline]
    fn check_guard(&self, guard: &Guard) {
        if let Some(c) = guard.collector() {
            assert_eq!(c, &self.collector);
        }
    }

    pub fn len(&self) -> usize {
        let n = self.count.load(Ordering::Relaxed);
        if n < 0 {
            0
        } else {
            n as usize
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[cfg(test)]
    fn capacity(&self, guard: &Guard) -> usize {
        self.check_guard(guard);
        let table = self.table.load(Ordering::Relaxed, &guard);

        if table.is_null() {
            0
        } else {
            unsafe { table.deref() }.len()
        }
    }

    fn resize_stamp(n: usize) -> isize {
        n.leading_zeros() as isize | (1_isize << (RESIZE_STAMP_BITS - 1))
    }

    pub fn iter<'g>(&'g self, guard: &'g Guard) -> Iter<'g, K, V> {
        self.check_guard(guard);
        let table = self.table.load(Ordering::SeqCst, guard);
        let node_iter = NodeIter::new(table, guard);
        Iter { node_iter, guard }
    }

    pub fn keys<'g>(&'g self, guard: &'g Guard) -> Keys<'g, K, V> {
        self.check_guard(guard);
        let table = self.table.load(Ordering::SeqCst, guard);
        let node_iter = NodeIter::new(table, guard);
        Keys { node_iter }
    }

    pub fn values<'g>(&'g self, guard: &'g Guard) -> Values<'g, K, V> {
        self.check_guard(guard);
        let table = self.table.load(Ordering::SeqCst, guard);
        let node_iter = NodeIter::new(table, guard);
        Values { node_iter, guard }
    }

    fn init_table<'g>(&'g self, guard: &'g Guard) -> Shared<'g, Table<K, V>> {
        loop {
            let table = self.table.load(Ordering::SeqCst, guard);
            if !table.is_null() && !unsafe { table.deref() }.is_empty() {
                break table;
            }
            let mut sc = self.size_ctl.load(Ordering::SeqCst);
            if sc < 0 {
                std::thread::yield_now();
                continue;
            }

            if self.size_ctl.compare_and_swap(sc, -1, Ordering::SeqCst) == sc {
                let mut table = self.table.load(Ordering::SeqCst, guard);

                if table.is_null() || unsafe { table.deref() }.is_empty() {
                    let n = if sc > 0 {
                        sc as usize
                    } else {
                        DEFAULT_CAPACITY
                    };
                    let new_table = Owned::new(Table::new(n));
                    table = new_table.into_shared(guard);
                    self.table.store(table, Ordering::SeqCst);
                    sc = load_factor!(n as isize)
                }
                self.size_ctl.store(sc, Ordering::SeqCst);
                break table;
            }
        }
    }

    fn presize(&mut self, size: usize) {
        let guard = unsafe { epoch::unprotected() };

        let requested_capacity = if size >= MAXIMUM_CAPACITY / 2 {
            MAXIMUM_CAPACITY
        } else {
            let size = size + (size >> 1) + 1;

            std::cmp::min(MAXIMUM_CAPACITY, size.next_power_of_two())
        } as usize;

        assert_eq!(self.size_ctl.load(Ordering::SeqCst), 0);
        assert!(self.table.load(Ordering::SeqCst, &guard).is_null());

        let new_table = Owned::new(Table::new(requested_capacity)).into_shared(guard);

        self.table.store(new_table, Ordering::SeqCst);

        let new_load_to_resize_at = load_factor!(requested_capacity as isize);

        self.size_ctl.store(new_load_to_resize_at, Ordering::SeqCst);
    }
}

impl<K, V, S> HashMap<K, V, S>
where
    K: Clone + Ord,
{
    fn try_presize(&self, size: usize, guard: &Guard) {
        let requested_capacity = if size >= MAXIMUM_CAPACITY / 2 {
            MAXIMUM_CAPACITY
        } else {
            let size = size + (size >> 1) + 1;

            std::cmp::min(MAXIMUM_CAPACITY, size.next_power_of_two())
        } as isize;

        loop {
            let size_ctl = self.size_ctl.load(Ordering::SeqCst);
            if size_ctl < 0 {
                break;
            }

            let table = self.table.load(Ordering::SeqCst, &guard);

            let current_capactity = if table.is_null() {
                0
            } else {
                unsafe { table.deref() }.len()
            };

            if current_capactity == 0 {
                let initial_capacity = size_ctl;

                let new_capacity = requested_capacity.max(initial_capacity) as usize;

                if self
                    .size_ctl
                    .compare_and_swap(size_ctl, -1, Ordering::SeqCst)
                    != size_ctl
                {
                    continue;
                }

                if self.table.load(Ordering::SeqCst, guard) != table {
                    self.size_ctl.store(size_ctl, Ordering::SeqCst);
                    continue;
                }

                let new_table = Owned::new(Table::new(new_capacity)).into_shared(guard);

                let old_table = self.table.swap(new_table, Ordering::SeqCst, &guard);

                assert!(old_table.is_null());

                let new_load_to_resize_at = load_factor!(new_capacity as isize);

                self.size_ctl.store(new_load_to_resize_at, Ordering::SeqCst);
            } else if requested_capacity <= size_ctl || current_capactity >= MAXIMUM_CAPACITY {
                break;
            } else if table == self.table.load(Ordering::SeqCst, &guard) {
                let rs: isize = Self::resize_stamp(current_capactity) << RESIZE_STAMP_SHIFT;

                if self
                    .size_ctl
                    .compare_and_swap(size_ctl, rs + 2, Ordering::SeqCst)
                    == size_ctl
                {
                    self.transfer(table, Shared::null(), &guard);
                }
            }
        }
    }

    #[inline(never)]
    fn transfer<'g>(
        &'g self,
        table: Shared<'g, Table<K, V>>,
        mut next_table: Shared<'g, Table<K, V>>,
        guard: &'g Guard,
    ) {
        let n = unsafe { table.deref() }.len();
        let ncpu = num_cpus();

        let stride = if ncpu > 1 { (n >> 3) / ncpu } else { n };
        let stride = std::cmp::max(stride as isize, MIN_TRANSFER_STRIDE);

        if next_table.is_null() {
            let table = Owned::new(Table::new(n << 1));
            let now_garbage = self.next_table.swap(table, Ordering::SeqCst, guard);
            assert!(now_garbage.is_null());
            self.transfer_index.store(n as isize, Ordering::SeqCst);
            next_table = self.next_table.load(Ordering::Relaxed, guard);
        }

        let next_n = unsafe { next_table.deref() }.len();

        let mut advance = true;
        let mut finishing = false;
        let mut i = 0;
        let mut bound = 0;
        loop {
            while advance {
                i -= 1;
                if i >= bound || finishing {
                    advance = false;
                    break;
                }

                let next_index = self.transfer_index.load(Ordering::SeqCst);
                if next_index <= 0 {
                    i = -1;
                    advance = false;
                    break;
                }

                let next_bound = if next_index > stride {
                    next_index - stride
                } else {
                    0
                };
                if self
                    .transfer_index
                    .compare_and_swap(next_index, next_bound, Ordering::SeqCst)
                    == next_index
                {
                    bound = next_bound;
                    i = next_index;
                    advance = false;
                    break;
                }
            }

            if i < 0 || i as usize >= n || i as usize + n >= next_n {
                if finishing {
                    self.next_table.store(Shared::null(), Ordering::SeqCst);
                    let now_garbage = self.table.swap(next_table, Ordering::SeqCst, guard);
                    unsafe { guard.defer_destroy(now_garbage) };
                    self.size_ctl
                        .store(((n as isize) << 1) - ((n as isize) >> 1), Ordering::SeqCst);
                    return;
                }

                let sc = self.size_ctl.load(Ordering::SeqCst);
                if self.size_ctl.compare_and_swap(sc, sc - 1, Ordering::SeqCst) == sc {
                    if (sc - 2) != Self::resize_stamp(n) << RESIZE_STAMP_SHIFT {
                        return;
                    }

                    finishing = true;

                    advance = true;

                    i = n as isize;
                }

                continue;
            }
            let i = i as usize;

            let table = unsafe { table.deref() };

            let bin = table.bin(i as usize, guard);
            if bin.is_null() {
                advance = table
                    .cas_bin(i, Shared::null(), table.get_moved(next_table, guard), guard)
                    .is_ok();
                continue;
            }
            let next_table = unsafe { next_table.deref() };

            match *unsafe { bin.deref() } {
                BinEntry::Moved => {
                    advance = true;
                }
                BinEntry::Node(ref head) => {
                    let head_lock = head.lock.lock();

                    let current_head = table.bin(i, guard);
                    if current_head.as_raw() != bin.as_raw() {
                        continue;
                    }

                    let mut run_bit = head.hash & n as u64;
                    let mut last_run = bin;
                    let mut p = bin;
                    loop {
                        let node = unsafe { p.deref() }.as_node().unwrap();
                        let next = node.next.load(Ordering::SeqCst, guard);

                        let b = node.hash & n as u64;
                        if b != run_bit {
                            run_bit = b;
                            last_run = p;
                        }

                        if next.is_null() {
                            break;
                        }
                        p = next;
                    }

                    let mut low_bin = Shared::null();
                    let mut high_bin = Shared::null();
                    if run_bit == 0 {
                        low_bin = last_run;
                    } else {
                        high_bin = last_run;
                    }

                    p = bin;
                    while p != last_run {
                        let node = unsafe { p.deref() }.as_node().unwrap();

                        let link = if node.hash & n as u64 == 0 {
                            &mut low_bin
                        } else {
                            &mut high_bin
                        };

                        *link = Owned::new(BinEntry::Node(Node::with_next(
                            node.hash,
                            node.key.clone(),
                            node.value.clone(),
                            Atomic::from(*link),
                        )))
                        .into_shared(guard);

                        p = node.next.load(Ordering::SeqCst, guard);
                    }

                    next_table.store_bin(i, low_bin);
                    next_table.store_bin(i + n, high_bin);
                    table.store_bin(
                        i,
                        table.get_moved(Shared::from(next_table as *const _), guard),
                    );

                    p = bin;
                    while p != last_run {
                        let next = unsafe { p.deref() }
                            .as_node()
                            .unwrap()
                            .next
                            .load(Ordering::SeqCst, guard);
                        unsafe { guard.defer_destroy(p) };
                        p = next;
                    }

                    advance = true;

                    drop(head_lock);
                }
                BinEntry::Tree(ref tree_bin) => {
                    let bin_lock = tree_bin.lock.lock();

                    let current_head = table.bin(i, guard);
                    if current_head != bin {
                        continue;
                    }

                    let mut low = Shared::null();
                    let mut low_tail = Shared::null();
                    let mut high = Shared::null();
                    let mut high_tail = Shared::null();
                    let mut low_count = 0;
                    let mut high_count = 0;
                    let mut e = tree_bin.first.load(Ordering::Relaxed, guard);
                    while !e.is_null() {
                        let tree_node = unsafe { TreeNode::get_tree_node(e) };
                        let hash = tree_node.node.hash;
                        let new_node = TreeNode::new(
                            hash,
                            tree_node.node.key.clone(),
                            tree_node.node.value.clone(),
                            Atomic::null(),
                            Atomic::null(),
                        );
                        let run_bit = hash & n as u64;
                        if run_bit == 0 {
                            new_node.prev.store(low_tail, Ordering::Relaxed);
                            let new_node =
                                Owned::new(BinEntry::TreeNode(new_node)).into_shared(guard);
                            if low_tail.is_null() {
                                low = new_node;
                            } else {
                                unsafe { TreeNode::get_tree_node(low_tail) }
                                    .node
                                    .next
                                    .store(new_node, Ordering::Relaxed);
                            }
                            low_tail = new_node;
                            low_count += 1;
                        } else {
                            new_node.prev.store(high_tail, Ordering::Relaxed);
                            let new_node =
                                Owned::new(BinEntry::TreeNode(new_node)).into_shared(guard);
                            if high_tail.is_null() {
                                high = new_node;
                            } else {
                                unsafe { TreeNode::get_tree_node(high_tail) }
                                    .node
                                    .next
                                    .store(new_node, Ordering::Relaxed);
                            }
                            high_tail = new_node;
                            high_count += 1;
                        }
                        e = tree_node.node.next.load(Ordering::Relaxed, guard);
                    }

                    let mut reused_bin = false;
                    let low_bin = if low_count <= UNTREEIFY_THRESHOLD {
                        let low_linear = Self::untreeify(low, guard);
                        unsafe { TreeBin::drop_tree_nodes(low, false, guard) };
                        low_linear
                    } else if high_count != 0 {
                        Owned::new(BinEntry::Tree(TreeBin::new(
                            unsafe { low.into_owned() },
                            guard,
                        )))
                        .into_shared(guard)
                    } else {
                        reused_bin = true;
                        unsafe { TreeBin::drop_tree_nodes(low, false, guard) };
                        bin
                    };
                    let high_bin = if high_count <= UNTREEIFY_THRESHOLD {
                        let high_linear = Self::untreeify(high, guard);
                        unsafe { TreeBin::drop_tree_nodes(high, false, guard) };
                        high_linear
                    } else if low_count != 0 {
                        Owned::new(BinEntry::Tree(TreeBin::new(
                            unsafe { high.into_owned() },
                            guard,
                        )))
                        .into_shared(guard)
                    } else {
                        reused_bin = true;
                        unsafe { TreeBin::drop_tree_nodes(high, false, guard) };
                        bin
                    };

                    next_table.store_bin(i, low_bin);
                    next_table.store_bin(i + n, high_bin);
                    table.store_bin(
                        i,
                        table.get_moved(Shared::from(next_table as *const _), guard),
                    );

                    if !reused_bin {
                        unsafe { TreeBin::defer_drop_without_values(bin, guard) };
                    }

                    advance = true;
                    drop(bin_lock);
                }
                BinEntry::TreeNode(_) => unreachable!(
                    "The head of a bin cannot be a TreeNode directly without BinEntry::Tree"
                ),
            }
        }
    }

    fn help_transfer<'g>(
        &'g self,
        table: Shared<'g, Table<K, V>>,
        guard: &'g Guard,
    ) -> Shared<'g, Table<K, V>> {
        if table.is_null() {
            return table;
        }

        let next_table = unsafe { table.deref() }.next_table(guard);
        if next_table.is_null() {
            return table;
        }

        let rs = Self::resize_stamp(unsafe { table.deref() }.len()) << RESIZE_STAMP_SHIFT;

        while next_table == self.next_table.load(Ordering::SeqCst, guard)
            && table == self.table.load(Ordering::SeqCst, guard)
        {
            let sc = self.size_ctl.load(Ordering::SeqCst);
            if sc >= 0
                || sc == rs + MAX_RESIZERS
                || sc == rs + 1
                || self.transfer_index.load(Ordering::SeqCst) <= 0
            {
                break;
            }

            if self.size_ctl.compare_and_swap(sc, sc + 1, Ordering::SeqCst) == sc {
                self.transfer(table, next_table, guard);
                break;
            }
        }
        next_table
    }

    fn add_count(&self, n: isize, resize_hint: Option<usize>, guard: &Guard) {
        use std::cmp;
        let mut count = match n.cmp(&0) {
            cmp::Ordering::Greater => self.count.fetch_add(n, Ordering::SeqCst) + n,
            cmp::Ordering::Less => self.count.fetch_sub(n.abs(), Ordering::SeqCst) - n,
            cmp::Ordering::Equal => self.count.load(Ordering::SeqCst),
        };

        if resize_hint.is_none() {
            return;
        }

        let _saw_bin_length = resize_hint.unwrap();

        loop {
            let sc = self.size_ctl.load(Ordering::SeqCst);
            if (count as isize) < sc {
                break;
            }

            let table = self.table.load(Ordering::SeqCst, guard);
            if table.is_null() {
                break;
            }

            let n = unsafe { table.deref() }.len();
            if n >= MAXIMUM_CAPACITY {
                break;
            }

            let rs = Self::resize_stamp(n) << RESIZE_STAMP_SHIFT;
            if sc < 0 {
                if sc == rs + MAX_RESIZERS || sc == rs + 1 {
                    break;
                }
                let nt = self.next_table.load(Ordering::SeqCst, guard);
                if nt.is_null() {
                    break;
                }
                if self.transfer_index.load(Ordering::SeqCst) <= 0 {
                    break;
                }

                if self.size_ctl.compare_and_swap(sc, sc + 1, Ordering::SeqCst) == sc {
                    self.transfer(table, nt, guard);
                }
            } else if self.size_ctl.compare_and_swap(sc, rs + 2, Ordering::SeqCst) == sc {
                self.transfer(table, Shared::null(), guard);
            }

            count = self.count.load(Ordering::SeqCst);
        }
    }

    pub fn reserve(&self, additional: usize, guard: &Guard) {
        self.check_guard(guard);
        let absolute = self.len() + additional;
        self.try_presize(absolute, guard);
    }
}

impl<K, V, S> HashMap<K, V, S>
where
    K: Hash + Ord,
    S: BuildHasher,
{
    #[inline]
    fn hash<Q: ?Sized + Hash>(&self, key: &Q) -> u64 {
        let mut h = self.build_hasher.build_hasher();
        key.hash(&mut h);
        h.finish()
    }

    fn get_node<'g, Q>(&'g self, key: &Q, guard: &'g Guard) -> Option<&'g Node<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        let table = self.table.load(Ordering::SeqCst, guard);
        if table.is_null() {
            return None;
        }

        let table = unsafe { table.deref() };
        if table.is_empty() {
            return None;
        }

        let h = self.hash(key);
        let bini = table.bini(h);
        let bin = table.bin(bini, guard);
        if bin.is_null() {
            return None;
        }

        let node = table.find(unsafe { bin.deref() }, h, key, guard);
        if node.is_null() {
            return None;
        }
        let node = unsafe { node.deref() };
        Some(match node {
            BinEntry::Node(ref n) => n,
            BinEntry::TreeNode(ref tn) => &tn.node,
            _ => panic!("`Table::find` should always return a Node"),
        })
    }

    pub fn contains_key<Q>(&self, key: &Q, guard: &Guard) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.check_guard(guard);
        self.get(key, &guard).is_some()
    }

    #[inline]
    pub fn get<'g, Q>(&'g self, key: &Q, guard: &'g Guard) -> Option<&'g V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.check_guard(guard);
        let node = self.get_node(key, guard)?;

        let v = node.value.load(Ordering::SeqCst, guard);
        assert!(!v.is_null());
        unsafe { v.as_ref() }
    }

    #[inline]
    pub fn get_key_value<'g, Q>(&'g self, key: &Q, guard: &'g Guard) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.check_guard(guard);
        let node = self.get_node(key, guard)?;

        let v = node.value.load(Ordering::SeqCst, guard);
        assert!(!v.is_null());
        unsafe { v.as_ref() }.map(|v| (&node.key, v))
    }

    pub(crate) fn guarded_eq(&self, other: &Self, our_guard: &Guard, their_guard: &Guard) -> bool
    where
        V: PartialEq,
    {
        if self.len() != other.len() {
            return false;
        }

        self.iter(our_guard)
            .all(|(key, value)| other.get(key, their_guard).map_or(false, |v| *value == *v))
    }
}

impl<K, V, S> HashMap<K, V, S>
where
    K: Clone + Ord,
{
    pub fn clear(&self, guard: &Guard) {
        let mut delta = 0;
        let mut idx = 0usize;

        let mut table = self.table.load(Ordering::SeqCst, guard);
        while !table.is_null() && idx < unsafe { table.deref() }.len() {
            let tab = unsafe { table.deref() };
            let raw_node = tab.bin(idx, guard);
            if raw_node.is_null() {
                idx += 1;
                continue;
            }
            match unsafe { raw_node.deref() } {
                BinEntry::Moved => {
                    table = self.help_transfer(table, guard);
                    idx = 0;
                }
                BinEntry::Node(ref node) => {
                    let head_lock = node.lock.lock();
                    let current_head = tab.bin(idx, guard);
                    if current_head != raw_node {
                        continue;
                    }
                    tab.store_bin(idx, Shared::null());
                    drop(head_lock);
                    let mut p = node.next.load(Ordering::SeqCst, guard);
                    while !p.is_null() {
                        delta -= 1;
                        p = {
                            let node = unsafe { p.deref() }
                                .as_node()
                                .expect("entry following Node should always be a Node");
                            let next = node.next.load(Ordering::SeqCst, guard);
                            let value = node.value.load(Ordering::SeqCst, guard);

                            unsafe { guard.defer_destroy(value) };
                            unsafe { guard.defer_destroy(p) };
                            next
                        };
                    }
                    let value = node.value.load(Ordering::SeqCst, guard);
                    unsafe { guard.defer_destroy(value) };
                    unsafe { guard.defer_destroy(raw_node) };
                    delta -= 1;
                    idx += 1;
                }
                BinEntry::Tree(ref tree_bin) => {
                    let bin_lock = tree_bin.lock.lock();
                    let current_head = tab.bin(idx, guard);
                    if current_head != raw_node {
                        continue;
                    }
                    tab.store_bin(idx, Shared::null());
                    drop(bin_lock);
                    let mut p = tree_bin.first.load(Ordering::SeqCst, guard);
                    while !p.is_null() {
                        delta -= 1;
                        p = {
                            let tree_node = unsafe { TreeNode::get_tree_node(p) };
                            tree_node.node.next.load(Ordering::SeqCst, guard)
                        };
                    }
                    unsafe { guard.defer_destroy(raw_node) };
                    idx += 1;
                }
                BinEntry::TreeNode(_) => unreachable!(
                    "The head of a bin cannot be a TreeNode directly without BinEntry::Tree"
                ),
            };
        }

        if delta != 0 {
            self.add_count(delta, None, guard);
        }
    }
}

impl<K, V, S> HashMap<K, V, S>
where
    K: 'static + Sync + Send + Clone + Hash + Ord,
    V: 'static + Sync + Send,
    S: BuildHasher,
{
    pub fn insert<'g>(&'g self, key: K, value: V, guard: &'g Guard) -> Option<&'g V> {
        self.check_guard(guard);
        self.put(key, value, false, guard).before()
    }

    #[inline]
    pub fn try_insert<'g>(
        &'g self,
        key: K,
        value: V,
        guard: &'g Guard,
    ) -> Result<&'g V, TryInsertError<'g, V>> {
        match self.put(key, value, true, guard) {
            PutResult::Exists {
                current,
                not_inserted,
            } => Err(TryInsertError {
                current,
                not_inserted: *not_inserted,
            }),
            PutResult::Inserted { new } => Ok(new),
            PutResult::Replaced { .. } => {
                unreachable!("no_replacement cannot result in PutResult::Replaced")
            }
        }
    }

    fn put<'g>(
        &'g self,
        mut key: K,
        value: V,
        no_replacement: bool,
        guard: &'g Guard,
    ) -> PutResult<'g, V> {
        let hash = self.hash(&key);
        let mut table = self.table.load(Ordering::SeqCst, guard);
        let mut bin_count;
        let value = Owned::new(value).into_shared(guard);
        let mut old_val = None;
        loop {
            if table.is_null() || unsafe { table.deref() }.is_empty() {
                table = self.init_table(guard);
                continue;
            }

            let t = unsafe { table.deref() };

            let bini = t.bini(hash);
            let mut bin = t.bin(bini, guard);
            if bin.is_null() {
                let node = Owned::new(BinEntry::Node(Node::new(hash, key, value)));
                match t.cas_bin(bini, bin, node, guard) {
                    Ok(_old_null_ptr) => {
                        self.add_count(1, Some(0), guard);
                        guard.flush();
                        return PutResult::Inserted {
                            new: unsafe { value.deref() },
                        };
                    }
                    Err(changed) => {
                        assert!(!changed.current.is_null());
                        bin = changed.current;
                        if let BinEntry::Node(node) = *changed.new.into_box() {
                            key = node.key;
                        } else {
                            unreachable!("we declared node and it is a BinEntry::Node");
                        }
                    }
                }
            }

            match *unsafe { bin.deref() } {
                BinEntry::Moved => {
                    table = self.help_transfer(table, guard);
                    continue;
                }
                BinEntry::Node(ref head)
                    if no_replacement && head.hash == hash && head.key == key =>
                {
                    let v = head.value.load(Ordering::SeqCst, guard);
                    return PutResult::Exists {
                        current: unsafe { v.deref() },
                        not_inserted: unsafe { value.into_owned().into_box() },
                    };
                }
                BinEntry::Node(ref head) => {
                    let head_lock = head.lock.lock();

                    let current_head = t.bin(bini, guard);
                    if current_head != bin {
                        continue;
                    }

                    bin_count = 1;
                    let mut p = bin;

                    old_val = loop {
                        let n = unsafe { p.deref() }.as_node().unwrap();
                        if n.hash == hash && n.key == key {
                            let current_value = n.value.load(Ordering::SeqCst, guard);

                            let current_value = unsafe { current_value.deref() };

                            if no_replacement {
                                return PutResult::Exists {
                                    current: current_value,
                                    not_inserted: unsafe { value.into_owned().into_box() },
                                };
                            } else {
                                let now_garbage = n.value.swap(value, Ordering::SeqCst, guard);

                                unsafe { guard.defer_destroy(now_garbage) };
                            }
                            break Some(current_value);
                        }

                        let next = n.next.load(Ordering::SeqCst, guard);
                        if next.is_null() {
                            let node = Owned::new(BinEntry::Node(Node::new(hash, key, value)));
                            n.next.store(node, Ordering::SeqCst);
                            break None;
                        }
                        p = next;

                        bin_count += 1;
                    };
                    drop(head_lock);
                }
                BinEntry::Tree(ref tree_bin) => {
                    let head_lock = tree_bin.lock.lock();

                    let current_head = t.bin(bini, guard);
                    if current_head != bin {
                        continue;
                    }

                    bin_count = 2;
                    let p = tree_bin.find_or_put_tree_val(hash, key, value, guard);
                    if p.is_null() {
                        break;
                    }
                    let tree_node = unsafe { TreeNode::get_tree_node(p) };
                    old_val = {
                        let current_value = tree_node.node.value.load(Ordering::SeqCst, guard);
                        let current_value = unsafe { current_value.deref() };
                        if no_replacement {
                            return PutResult::Exists {
                                current: current_value,
                                not_inserted: unsafe { value.into_owned().into_box() },
                            };
                        } else {
                            let now_garbage =
                                tree_node.node.value.swap(value, Ordering::SeqCst, guard);

                            unsafe { guard.defer_destroy(now_garbage) };
                        }
                        Some(current_value)
                    };
                    drop(head_lock);
                }
                BinEntry::TreeNode(_) => unreachable!(
                    "The head of a bin cannot be a TreeNode directly without BinEntry::Tree"
                ),
            }
            debug_assert_ne!(bin_count, 0);
            if bin_count >= TREEIFY_THRESHOLD {
                self.treeify_bin(t, bini, guard);
            }
            if let Some(old_val) = old_val {
                return PutResult::Replaced {
                    old: old_val,
                    new: unsafe { value.deref() },
                };
            }
            break;
        }
        debug_assert!(old_val.is_none());
        self.add_count(1, Some(bin_count), guard);
        guard.flush();
        PutResult::Inserted {
            new: unsafe { value.deref() },
        }
    }

    fn put_all<I: Iterator<Item = (K, V)>>(&self, iter: I, guard: &Guard) {
        for (key, value) in iter {
            self.put(key, value, false, guard);
        }
    }

    pub fn compute_if_present<'g, Q, F>(
        &'g self,
        key: &Q,
        remapping_function: F,
        guard: &'g Guard,
    ) -> Option<&'g V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
        F: FnOnce(&K, &V) -> Option<V>,
    {
        self.check_guard(guard);
        let hash = self.hash(&key);

        let mut table = self.table.load(Ordering::SeqCst, guard);
        let mut new_val = None;
        let mut removed_node = false;
        let mut bin_count;
        loop {
            if table.is_null() || unsafe { table.deref() }.is_empty() {
                table = self.init_table(guard);
                continue;
            }

            let t = unsafe { table.deref() };

            let bini = t.bini(hash);
            let bin = t.bin(bini, guard);
            if bin.is_null() {
                return None;
            }

            match *unsafe { bin.deref() } {
                BinEntry::Moved => {
                    table = self.help_transfer(table, guard);
                    continue;
                }
                BinEntry::Node(ref head) => {
                    let head_lock = head.lock.lock();

                    let current_head = t.bin(bini, guard);
                    if current_head != bin {
                        continue;
                    }

                    bin_count = 1;
                    let mut p = bin;
                    let mut pred: Shared<'_, BinEntry<K, V>> = Shared::null();

                    new_val = loop {
                        let n = unsafe { p.deref() }.as_node().unwrap();
                        let next = n.next.load(Ordering::SeqCst, guard);
                        if n.hash == hash && n.key.borrow() == key {
                            let current_value = n.value.load(Ordering::SeqCst, guard);

                            let new_value =
                                remapping_function(&n.key, unsafe { current_value.deref() });

                            if let Some(value) = new_value {
                                let value = Owned::new(value).into_shared(guard);
                                let now_garbage = n.value.swap(value, Ordering::SeqCst, guard);

                                unsafe { guard.defer_destroy(now_garbage) };

                                break Some(unsafe { value.deref() });
                            } else {
                                removed_node = true;
                                if !pred.is_null() {
                                    unsafe { pred.deref() }
                                        .as_node()
                                        .unwrap()
                                        .next
                                        .store(next, Ordering::SeqCst);
                                } else {
                                    t.store_bin(bini, next);
                                }

                                unsafe { guard.defer_destroy(p) };
                                unsafe { guard.defer_destroy(current_value) };
                                break None;
                            }
                        }

                        pred = p;
                        if next.is_null() {
                            break None;
                        }
                        p = next;

                        bin_count += 1;
                    };
                    drop(head_lock);
                }
                BinEntry::Tree(ref tree_bin) => {
                    let bin_lock = tree_bin.lock.lock();

                    let current_head = t.bin(bini, guard);
                    if current_head != bin {
                        continue;
                    }

                    bin_count = 2;
                    let root = tree_bin.root.load(Ordering::SeqCst, guard);
                    if root.is_null() {
                        break;
                    }
                    new_val = {
                        let p = TreeNode::find_tree_node(root, hash, key, guard);
                        if p.is_null() {
                            None
                        } else {
                            let n = &unsafe { TreeNode::get_tree_node(p) }.node;
                            let current_value = n.value.load(Ordering::SeqCst, guard);

                            let new_value =
                                remapping_function(&n.key, unsafe { current_value.deref() });

                            if let Some(value) = new_value {
                                let value = Owned::new(value).into_shared(guard);
                                let now_garbage = n.value.swap(value, Ordering::SeqCst, guard);

                                unsafe { guard.defer_destroy(now_garbage) };
                                Some(unsafe { value.deref() })
                            } else {
                                removed_node = true;
                                let need_to_untreeify =
                                    unsafe { tree_bin.remove_tree_node(p, true, guard) };
                                if need_to_untreeify {
                                    let linear_bin = Self::untreeify(
                                        tree_bin.first.load(Ordering::SeqCst, guard),
                                        guard,
                                    );
                                    t.store_bin(bini, linear_bin);
                                    unsafe {
                                        TreeBin::defer_drop_without_values(bin, guard);
                                        guard.defer_destroy(p);
                                        guard.defer_destroy(current_value);
                                    }
                                }
                                None
                            }
                        }
                    };
                    drop(bin_lock);
                }
                BinEntry::TreeNode(_) => unreachable!(
                    "The head of a bin cannot be a TreeNode directly without BinEntry::Tree"
                ),
            }
            debug_assert_ne!(bin_count, 0);
            break;
        }
        if removed_node {
            self.add_count(-1, Some(bin_count), guard);
        }
        guard.flush();
        new_val
    }

    pub fn remove<'g, Q>(&'g self, key: &Q, guard: &'g Guard) -> Option<&'g V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.check_guard(guard);
        self.replace_node(key, None, None, guard).map(|(_, v)| v)
    }

    pub fn remove_entry<'g, Q>(&'g self, key: &Q, guard: &'g Guard) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        self.check_guard(guard);
        self.replace_node(key, None, None, guard)
    }

    fn replace_node<'g, Q>(
        &'g self,
        key: &Q,
        new_value: Option<V>,
        observed_value: Option<Shared<'g, V>>,
        guard: &'g Guard,
    ) -> Option<(&'g K, &'g V)>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Ord,
    {
        let hash = self.hash(key);

        let is_remove = new_value.is_none();
        let mut old_val = None;
        let mut table = self.table.load(Ordering::SeqCst, guard);
        loop {
            if table.is_null() {
                break;
            }

            let t = unsafe { table.deref() };
            let n = t.len() as u64;
            if n == 0 {
                break;
            }
            let bini = t.bini(hash);
            let bin = t.bin(bini, guard);
            if bin.is_null() {
                break;
            }

            match *unsafe { bin.deref() } {
                BinEntry::Moved => {
                    table = self.help_transfer(table, guard);
                    continue;
                }
                BinEntry::Node(ref head) => {
                    let head_lock = head.lock.lock();

                    if t.bin(bini, guard) != bin {
                        continue;
                    }

                    let mut e = bin;
                    let mut pred: Shared<'_, BinEntry<K, V>> = Shared::null();
                    loop {
                        let n = unsafe { e.deref() }.as_node().unwrap();
                        let next = n.next.load(Ordering::SeqCst, guard);
                        if n.hash == hash && n.key.borrow() == key {
                            let ev = n.value.load(Ordering::SeqCst, guard);

                            if observed_value.map(|ov| ov == ev).unwrap_or(true) {
                                old_val = Some((&n.key, ev));

                                if let Some(nv) = new_value {
                                    n.value.store(Owned::new(nv), Ordering::SeqCst);
                                    break;
                                }
                                if !pred.is_null() {
                                    unsafe { pred.deref() }
                                        .as_node()
                                        .unwrap()
                                        .next
                                        .store(next, Ordering::SeqCst);
                                } else {
                                    t.store_bin(bini, next);
                                }

                                unsafe { guard.defer_destroy(e) };
                            }
                            break;
                        }
                        pred = e;
                        if next.is_null() {
                            break;
                        } else {
                            e = next;
                        }
                    }
                    drop(head_lock);
                }
                BinEntry::Tree(ref tree_bin) => {
                    let bin_lock = tree_bin.lock.lock();

                    if t.bin(bini, guard) != bin {
                        continue;
                    }

                    let root = tree_bin.root.load(Ordering::SeqCst, guard);
                    if root.is_null() {
                        break;
                    }
                    let p = TreeNode::find_tree_node(root, hash, key, guard);
                    if p.is_null() {
                        break;
                    }
                    let n = &unsafe { TreeNode::get_tree_node(p) }.node;
                    let pv = n.value.load(Ordering::SeqCst, guard);

                    if observed_value.map(|ov| ov == pv).unwrap_or(true) {
                        old_val = Some((&n.key, pv));

                        if let Some(nv) = new_value {
                            n.value.store(Owned::new(nv), Ordering::SeqCst);
                        } else {
                            let need_to_untreeify =
                                unsafe { tree_bin.remove_tree_node(p, false, guard) };
                            if need_to_untreeify {
                                let linear_bin = Self::untreeify(
                                    tree_bin.first.load(Ordering::SeqCst, guard),
                                    guard,
                                );
                                t.store_bin(bini, linear_bin);
                                unsafe {
                                    TreeBin::defer_drop_without_values(bin, guard);
                                    guard.defer_destroy(p);
                                }
                            }
                        }
                    }

                    drop(bin_lock);
                }
                BinEntry::TreeNode(_) => unreachable!(
                    "The head of a bin cannot be a TreeNode directly without BinEntry::Tree"
                ),
            }
            if let Some((key, val)) = old_val {
                if is_remove {
                    self.add_count(-1, None, guard);
                }

                unsafe { guard.defer_destroy(val) };

                return unsafe { val.as_ref() }.map(move |v| (key, v));
            }
            break;
        }
        None
    }

    pub fn retain<F>(&self, mut f: F, guard: &Guard)
    where
        F: FnMut(&K, &V) -> bool,
    {
        self.check_guard(guard);
        for (k, v) in self.iter(guard) {
            if !f(k, v) {
                let old_value: Shared<'_, V> = Shared::from(v as *const V);
                self.replace_node(k, None, Some(old_value), guard);
            }
        }
    }

    pub fn retain_force<F>(&self, mut f: F, guard: &Guard)
    where
        F: FnMut(&K, &V) -> bool,
    {
        self.check_guard(guard);
        for (k, v) in self.iter(guard) {
            if !f(k, v) {
                self.replace_node(k, None, None, guard);
            }
        }
    }
}

impl<K, V, S> HashMap<K, V, S>
where
    K: Clone + Ord,
{
    fn treeify_bin<'g>(&'g self, tab: &Table<K, V>, index: usize, guard: &'g Guard) {
        let n = tab.len();
        if n < MIN_TREEIFY_CAPACITY {
            self.try_presize(n << 1, guard);
        } else {
            let bin = tab.bin(index, guard);
            if bin.is_null() {
                return;
            }
            match unsafe { bin.deref() } {
                BinEntry::Node(ref node) => {
                    let lock = node.lock.lock();
                    if tab.bin(index, guard) != bin {
                        return;
                    }
                    let mut e = bin;
                    let mut head = Shared::null();
                    let mut tail = Shared::null();
                    while !e.is_null() {
                        let e_deref = unsafe { e.deref() }.as_node().unwrap();
                        let new_tree_node = TreeNode::new(
                            e_deref.hash,
                            e_deref.key.clone(),
                            e_deref.value.clone(),
                            Atomic::null(),
                            Atomic::null(),
                        );
                        new_tree_node.prev.store(tail, Ordering::Relaxed);
                        let new_tree_node =
                            Owned::new(BinEntry::TreeNode(new_tree_node)).into_shared(guard);
                        if tail.is_null() {
                            head = new_tree_node;
                        } else {
                            unsafe { tail.deref() }
                                .as_tree_node()
                                .unwrap()
                                .node
                                .next
                                .store(new_tree_node, Ordering::Relaxed);
                        }
                        tail = new_tree_node;
                        e = e_deref.next.load(Ordering::SeqCst, guard);
                    }
                    tab.store_bin(
                        index,
                        Owned::new(BinEntry::Tree(TreeBin::new(
                            unsafe { head.into_owned() },
                            guard,
                        ))),
                    );
                    drop(lock);
                    e = bin;
                    while !e.is_null() {
                        unsafe {
                            guard.defer_destroy(e);
                            e = e
                                .deref()
                                .as_node()
                                .unwrap()
                                .next
                                .load(Ordering::SeqCst, guard);
                        }
                    }
                }
                BinEntry::Moved | BinEntry::Tree(_) => {}
                BinEntry::TreeNode(_) => unreachable!("TreeNode cannot be the head of a bin"),
            }
        }
    }

    fn untreeify<'g>(
        bin: Shared<'g, BinEntry<K, V>>,
        guard: &'g Guard,
    ) -> Shared<'g, BinEntry<K, V>> {
        let mut head = Shared::null();
        let mut tail: Shared<'_, BinEntry<K, V>> = Shared::null();
        let mut q = bin;
        while !q.is_null() {
            let q_deref = unsafe { q.deref() }.as_tree_node().unwrap();
            let new_node = Owned::new(BinEntry::Node(Node::new(
                q_deref.node.hash,
                q_deref.node.key.clone(),
                q_deref.node.value.clone(),
            )))
            .into_shared(guard);
            if tail.is_null() {
                head = new_node;
            } else {
                unsafe { tail.deref() }
                    .as_node()
                    .unwrap()
                    .next
                    .store(new_node, Ordering::Relaxed);
            }
            tail = new_node;
            q = q_deref.node.next.load(Ordering::Relaxed, guard);
        }

        head
    }
}
impl<K, V, S> PartialEq for HashMap<K, V, S>
where
    K: Ord + Hash,
    V: PartialEq,
    S: BuildHasher,
{
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        self.guarded_eq(other, &self.guard(), &other.guard())
    }
}

impl<K, V, S> Eq for HashMap<K, V, S>
where
    K: Ord + Hash,
    V: Eq,
    S: BuildHasher,
{
}

impl<K, V, S> fmt::Debug for HashMap<K, V, S>
where
    K: Debug,
    V: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let guard = self.collector.register().pin();
        f.debug_map().entries(self.iter(&guard)).finish()
    }
}

impl<K, V, S> Drop for HashMap<K, V, S> {
    fn drop(&mut self) {
        let guard = unsafe { crossbeam_epoch::unprotected() };

        assert!(self.next_table.load(Ordering::SeqCst, guard).is_null());
        let table = self.table.swap(Shared::null(), Ordering::SeqCst, guard);
        if table.is_null() {
            return;
        }

        let mut table = unsafe { table.into_owned() }.into_box();
        table.drop_bins();
    }
}

impl<K, V, S> Extend<(K, V)> for &HashMap<K, V, S>
where
    K: 'static + Sync + Send + Clone + Hash + Ord,
    V: 'static + Sync + Send,
    S: BuildHasher,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        let iter = iter.into_iter();
        let reserve = if self.is_empty() {
            iter.size_hint().0
        } else {
            (iter.size_hint().0 + 1) / 2
        };

        let guard = self.collector.register().pin();
        self.reserve(reserve, &guard);
        (*self).put_all(iter, &guard);
    }
}

impl<'a, K, V, S> Extend<(&'a K, &'a V)> for &HashMap<K, V, S>
where
    K: 'static + Sync + Send + Copy + Hash + Ord,
    V: 'static + Sync + Send + Copy,
    S: BuildHasher,
{
    fn extend<T: IntoIterator<Item = (&'a K, &'a V)>>(&mut self, iter: T) {
        self.extend(iter.into_iter().map(|(&key, &value)| (key, value)));
    }
}

impl<K, V, S> FromIterator<(K, V)> for HashMap<K, V, S>
where
    K: 'static + Sync + Send + Clone + Hash + Ord,
    V: 'static + Sync + Send,
    S: BuildHasher + Default,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut iter = iter.into_iter();

        if let Some((key, value)) = iter.next() {
            let guard = unsafe { crossbeam_epoch::unprotected() };

            let (lower, _) = iter.size_hint();
            let map = HashMap::with_capacity_and_hasher(lower.saturating_add(1), S::default());

            map.put(key, value, false, &guard);
            map.put_all(iter, &guard);
            map
        } else {
            Self::default()
        }
    }
}

impl<'a, K, V, S> FromIterator<(&'a K, &'a V)> for HashMap<K, V, S>
where
    K: 'static + Sync + Send + Copy + Hash + Ord,
    V: 'static + Sync + Send + Copy,
    S: BuildHasher + Default,
{
    fn from_iter<T: IntoIterator<Item = (&'a K, &'a V)>>(iter: T) -> Self {
        Self::from_iter(iter.into_iter().map(|(&k, &v)| (k, v)))
    }
}

impl<'a, K, V, S> FromIterator<&'a (K, V)> for HashMap<K, V, S>
where
    K: 'static + Sync + Send + Copy + Hash + Ord,
    V: 'static + Sync + Send + Copy,
    S: BuildHasher + Default,
{
    fn from_iter<T: IntoIterator<Item = &'a (K, V)>>(iter: T) -> Self {
        Self::from_iter(iter.into_iter().map(|&(k, v)| (k, v)))
    }
}

impl<K, V, S> Clone for HashMap<K, V, S>
where
    K: 'static + Sync + Send + Clone + Hash + Ord,
    V: 'static + Sync + Send + Clone,
    S: BuildHasher + Clone,
{
    fn clone(&self) -> HashMap<K, V, S> {
        let cloned_map = Self::with_capacity_and_hasher(self.len(), self.build_hasher.clone());
        {
            let guard = self.collector.register().pin();
            for (k, v) in self.iter(&guard) {
                cloned_map.insert(k.clone(), v.clone(), &guard);
            }
        }
        cloned_map
    }
}

#[cfg(not(miri))]
#[inline]
fn num_cpus() -> usize {
    NCPU_INITIALIZER.call_once(|| NCPU.store(num_cpus::get_physical(), Ordering::Relaxed));
    NCPU.load(Ordering::Relaxed)
}