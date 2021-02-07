use crate::node::*;
use crossbeam_epoch::{Atomic, Guard, Owned, Pointer, Shared};
use std::borrow::Borrow;
use std::fmt::Debug;
use std::sync::atomic::Ordering;

#[derive(Debug)]
pub(crate) struct Table<K, V> {
    bins: Box<[Atomic<BinEntry<K, V>>]>,

    moved: Atomic<BinEntry<K, V>>,

    next_table: Atomic<Table<K, V>>,
}

impl<K, V> From<Vec<Atomic<BinEntry<K, V>>>> for Table<K, V> {
    fn from(bins: Vec<Atomic<BinEntry<K, V>>>) -> Self {
        Self {
            bins: bins.into_boxed_slice(),
            moved: Atomic::from(Owned::new(BinEntry::Moved)),
            next_table: Atomic::null(),
        }
    }
}

impl<K, V> Table<K, V> {
    pub(crate) fn new(bins: usize) -> Self {
        Self::from(vec![Atomic::null(); bins])
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.bins.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.bins.len()
    }

    pub(crate) fn get_moved<'g>(
        &'g self,
        for_table: Shared<'g, Table<K, V>>,
        guard: &'g Guard,
    ) -> Shared<'g, BinEntry<K, V>> {
        match self.next_table(guard) {
            t if t.is_null() => {
                match self.next_table.compare_and_set(
                    Shared::null(),
                    for_table,
                    Ordering::SeqCst,
                    guard,
                ) {
                    Ok(_) => {}
                    Err(changed) => {
                        assert!(!changed.current.is_null());
                        assert_eq!(changed.current, for_table);
                    }
                }
            }
            t => {
                assert_eq!(t, for_table);
            }
        }
        self.moved.load(Ordering::SeqCst, guard)
    }

    pub(crate) fn find<'g, Q>(
        &'g self,
        bin: &BinEntry<K, V>,
        hash: u64,
        key: &Q,
        guard: &'g Guard,
    ) -> Shared<'g, BinEntry<K, V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Ord,
    {
        match *bin {
            BinEntry::Node(_) => {
                let mut node = bin;
                loop {
                    let n = if let BinEntry::Node(ref n) = node {
                        n
                    } else {
                        unreachable!("BinEntry::Node only points to BinEntry::Node");
                    };

                    if n.hash == hash && n.key.borrow() == key {
                        return Shared::from(node as *const _);
                    }
                    let next = n.next.load(Ordering::SeqCst, guard);
                    if next.is_null() {
                        return Shared::null();
                    }
                    node = unsafe { next.deref() };
                }
            }
            BinEntry::Moved => {
                let mut table = unsafe { self.next_table(guard).deref() };

                loop {
                    if table.is_empty() {
                        return Shared::null();
                    }
                    let bini = table.bini(hash);
                    let bin = table.bin(bini, guard);
                    if bin.is_null() {
                        return Shared::null();
                    }
                    let bin = unsafe { bin.deref() };

                    match *bin {
                        BinEntry::Node(_) | BinEntry::Tree(_) => {
                            break table.find(bin, hash, key, guard)
                        }
                        BinEntry::Moved => {
                                                        table = unsafe { table.next_table(guard).deref() };
                            continue;
                        }
                        BinEntry::TreeNode(_) => unreachable!("`find` was called on a Moved entry pointing to a TreeNode, which cannot be the first entry in a bin"),
                    }
                }
            }
            BinEntry::TreeNode(_) => {
                unreachable!(
                    "`find` was called on a TreeNode, which cannot be the first entry in a bin"
                );
            }
            BinEntry::Tree(_) => TreeBin::find(Shared::from(bin as *const _), hash, key, guard),
        }
    }

    pub(crate) fn drop_bins(&mut self) {
        let guard = unsafe { crossbeam_epoch::unprotected() };

        for bin in Vec::from(std::mem::replace(&mut self.bins, vec![].into_boxed_slice())) {
            if bin.load(Ordering::SeqCst, guard).is_null() {
                continue;
            }

            let bin_entry = unsafe { bin.load(Ordering::SeqCst, guard).deref() };
            match *bin_entry {
                BinEntry::Moved => {}
                BinEntry::Node(_) => {
                    let mut p = unsafe { bin.into_owned() };
                    loop {
                        let node = if let BinEntry::Node(node) = *p.into_box() {
                            node
                        } else {
                            unreachable!();
                        };

                        let _ = unsafe { node.value.into_owned() };

                        if node.next.load(Ordering::SeqCst, guard).is_null() {
                            break;
                        }
                        p = unsafe { node.next.into_owned() };
                    }
                }
                BinEntry::Tree(_) => {
                    let p = unsafe { bin.into_owned() };
                    let bin = if let BinEntry::Tree(bin) = *p.into_box() {
                        bin
                    } else {
                        unreachable!();
                    };
                    drop(bin);
                }
                BinEntry::TreeNode(_) => unreachable!(
                    "The head of a bin cannot be a TreeNode directly without BinEntry::Tree"
                ),
            }
        }
    }
}

impl<K, V> Drop for Table<K, V> {
    fn drop(&mut self) {
        let guard = unsafe { crossbeam_epoch::unprotected() };

        let bins = Vec::from(std::mem::replace(&mut self.bins, vec![].into_boxed_slice()));

        if cfg!(debug_assertions) {
            for bin in bins.iter() {
                let bin = bin.load(Ordering::SeqCst, guard);
                if bin.is_null() {
                    continue;
                } else {
                    let bin = unsafe { bin.deref() };
                    if let BinEntry::Moved = *bin {
                    } else {
                        unreachable!("dropped table with non-empty bin");
                    }
                }
            }
        }

        drop(bins);

        let moved = self.moved.swap(Shared::null(), Ordering::SeqCst, guard);
        assert!(
            !moved.is_null(),
            "self.moved is initialized together with the table"
        );

        let moved = unsafe { moved.into_owned() };
        drop(moved);
    }
}

impl<K, V> Table<K, V> {
    #[inline]
    pub(crate) fn bini(&self, hash: u64) -> usize {
        let mask = self.bins.len() as u64 - 1;
        (hash & mask) as usize
    }

    #[inline]
    pub(crate) fn bin<'g>(&'g self, i: usize, guard: &'g Guard) -> Shared<'g, BinEntry<K, V>> {
        self.bins[i].load(Ordering::Acquire, guard)
    }

    #[inline]
    #[allow(clippy::type_complexity)]
    pub(crate) fn cas_bin<'g, P>(
        &'g self,
        i: usize,
        current: Shared<'_, BinEntry<K, V>>,
        new: P,
        guard: &'g Guard,
    ) -> Result<
        Shared<'g, BinEntry<K, V>>,
        crossbeam_epoch::CompareAndSetError<'g, BinEntry<K, V>, P>,
    >
    where
        P: Pointer<BinEntry<K, V>>,
    {
        self.bins[i].compare_and_set(current, new, Ordering::AcqRel, guard)
    }

    #[inline]
    pub(crate) fn store_bin<P: Pointer<BinEntry<K, V>>>(&self, i: usize, new: P) {
        self.bins[i].store(new, Ordering::Release)
    }

    #[inline]
    pub(crate) fn next_table<'g>(&'g self, guard: &'g Guard) -> Shared<'g, Table<K, V>> {
        self.next_table.load(Ordering::SeqCst, guard)
    }
}
