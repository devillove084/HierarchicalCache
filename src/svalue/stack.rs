#![allow(unused_imports)]

use crate::chashmap;

use super::types::{
    Key, 
    ReturnValue, 
    Value, 
    Count,
    KeyStack
};
use serde::{Serialize, Deserialize};
use crossbeam_epoch::{Atomic, Guard, Owned, Shared};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Default)]
pub struct Stack<T> {
    inner: Vec<T>,
}

impl<T: Clone> Stack<T> {
    pub fn new() -> Stack<T> {
        Stack { inner: Vec::new() }
    }

    pub fn push(&mut self, item: T) -> Count {
        self.inner.push(item);
        self.inner.len() as Count
    }

    pub fn pop(&mut self) -> Option<T> {
        self.inner.pop()
    }

    pub fn peek(&self) -> Option<T> {
        self.inner.last().cloned()
    }

    pub fn size(&self) -> Count {
        self.inner.len() as Count
    }
}

type RealStack = KeyStack;

trait StackOpt {
    fn stpush(&self, k: Key, v: Value) -> ReturnValue;
    fn stpop(&self, k: Key) -> ReturnValue;
    fn stpeek(&self, k: Key) -> ReturnValue;
    fn stsize(&self, k: Key) -> ReturnValue;
}

impl<T> StackOpt for Stack<T> {
    fn stpush(&self, k: Key, v: Value) -> ReturnValue {

        let spre = KeyStack::new();
        let g = spre.guard();
        let mut sbrk = Stack::new();
        sbrk.push(v);
        spre.insert(k, sbrk, &g);
        return ReturnValue::Ok;
    }

    fn stpeek(&self, k: Key) -> ReturnValue {
        return ReturnValue::Ok;
    }
    fn stpop(&self, k: Key) -> ReturnValue {
        return ReturnValue::Ok
    }
    fn stsize(&self, k: Key) -> ReturnValue {
        return ReturnValue::Ok;
    }
}
