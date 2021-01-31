use crate::node::{BinEntry, Node, TreeNode};
use crate::raw::Table;
use crossbeam_epoch::{Guard, Shared};
use std::sync::atomic::Ordering;

#[derive(Debug)]
pub(crate) struct NodeIter<'g, K, V> {
    table: Option<&'g Table<K, V>>,

    stack: Option<Box<TableStack<'g, K, V>>>,
    spare: Option<Box<TableStack<'g, K, V>>>,

    prev: Option<&'g Node<K, V>>,

    index: usize,

    base_index: usize,

    base_limit: usize,

    base_size: usize,

    guard: &'g Guard,
}

impl<'g, K, V> NodeIter<'g, K, V> {
    pub(crate) fn new(table: Shared<'g, Table<K, V>>, guard: &'g Guard) -> Self {
        let (table, len) = if table.is_null() {
            (None, 0)
        } else {
            let table = unsafe { table.deref() };
            (Some(table), table.len())
        };

        Self {
            table,
            stack: None,
            spare: None,
            prev: None,
            base_size: len,
            base_index: 0,
            index: 0,
            base_limit: len,
            guard,
        }
    }

    fn push_state(&mut self, t: &'g Table<K, V>, i: usize, n: usize) {
        let mut s = self.spare.take();
        if let Some(ref mut s) = s {
            self.spare = s.next.take();
        }

        let target = TableStack {
            table: t,
            length: n,
            index: i,
            next: self.stack.take(),
        };

        self.stack = if let Some(mut s) = s {
            *s = target;
            Some(s)
        } else {
            Some(Box::new(target))
        };
    }

    fn recover_state(&mut self, mut n: usize) {
        while let Some(ref mut s) = self.stack {
            if self.index + s.length < n {
                self.index += s.length;
                break;
            }

            let mut s = self.stack.take().expect("while let Some");
            n = s.length;
            self.index = s.index;
            self.table = Some(s.table);
            self.stack = s.next.take();

            s.next = self.spare.take();
            self.spare = Some(s);
        }

        if self.stack.is_none() {
            self.index += self.base_size;
            if self.index >= n {
                self.base_index += 1;
                self.index = self.base_index;
            }
        }
    }
}

impl<'g, K, V> Iterator for NodeIter<'g, K, V> {
    type Item = &'g Node<K, V>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut e = None;
        if let Some(prev) = self.prev {
            let next = prev.next.load(Ordering::SeqCst, self.guard);
            if !next.is_null() {
                match unsafe { next.deref() } {
                    BinEntry::Node(node) => {
                        e = Some(node);
                    }
                    BinEntry::TreeNode(tree_node) => {
                        e = Some(&tree_node.node);
                    }
                    BinEntry::Moved => unreachable!("Nodes can only point to Nodes or TreeNodes"),
                    BinEntry::Tree(_) => unreachable!("Nodes can only point to Nodes or TreeNodes"),
                }
            }
        }

        loop {
            if e.is_some() {
                self.prev = e;
                return e;
            }

            if self.base_index >= self.base_limit
                || self.table.is_none()
                || self.table.as_ref().unwrap().len() <= self.index
            {
                self.prev = None;
                return None;
            }

            let t = self.table.expect("is_none in if above");
            let i = self.index;
            let n = t.len();
            let bin = t.bin(i, self.guard);
            if !bin.is_null() {
                let bin = unsafe { bin.deref() };
                match bin {
                    BinEntry::Moved => {
                        self.table = Some(unsafe { t.next_table(self.guard).deref() });
                        self.prev = None;
                        self.push_state(t, i, n);
                        continue;
                    }
                    BinEntry::Node(node) => {
                        e = Some(node);
                    }
                    BinEntry::Tree(tree_bin) => {
                        e = Some(
                            &unsafe {
                                TreeNode::get_tree_node(
                                    tree_bin.first.load(Ordering::SeqCst, self.guard),
                                )
                            }
                            .node,
                        );
                    }
                    BinEntry::TreeNode(_) => unreachable!(
                        "The head of a bin cannot be a TreeNode directly without BinEntry::Tree"
                    ),
                }
            }

            if self.stack.is_some() {
                self.recover_state(n);
            } else {
                self.index = i + self.base_size;
                if self.index >= n {
                    self.base_index += 1;
                    self.index = self.base_index;
                }
            }
        }
    }
}

#[derive(Debug)]
struct TableStack<'g, K, V> {
    length: usize,
    index: usize,
    table: &'g Table<K, V>,
    next: Option<Box<TableStack<'g, K, V>>>,
}