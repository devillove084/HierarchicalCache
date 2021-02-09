#![allow(unused_imports)]
#![allow(unused_variables)]

use std::borrow::{Borrow, BorrowMut};

use crate::chashmap;

use super::types::{
    Key, 
    ReturnValue, 
    Value, 
    Count,
    KeyStack,
    StateRef
};
use crate::op_variants;
use bytes::Bytes;
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

op_variants! {
    StackOps,
    STPush(Key, Value),
    STPop(Key),
    STPeek(Key),
    STSize(Key)
}

//make_reader!(stacks, read_stacks);

pub async fn stack_interact(stack_op: StackOps, state: StateRef) -> ReturnValue {
    match stack_op {
        //StackOps::STPush(key, value) => state.stacks.BinEntry(key).or_default().push(value).into(),
        StackOps::STPush(key, value) =>  stpush(key, value),
        StackOps::STPop(key) => stpop(key),
        StackOps::STPeek(key) => stpeek(key),
        StackOps::STSize(key) => stsize(key),
        // StackOps::STPop(key) => state
        //     .stacks
        //     .get_mut(&key)
        //     .and_then(|mut st| st.pop())
        //     .map(ReturnValue::StringRes)
        //     .unwrap_or(ReturnValue::Nil),
        // StackOps::STPeek(key) => read_stacks!(state, &key)
        //     .and_then(|st| st.peek())
        //     .map(ReturnValue::StringRes)
        //     .unwrap_or(ReturnValue::Nil),
        // StackOps::STSize(key) => read_stacks!(state, &key)
        //     .map(|st| st.size())
        //     .map(ReturnValue::IntRes)
        //     .unwrap_or(ReturnValue::Nil),
    }
}

fn stpush(k: Key, v: Value) -> ReturnValue {
    let mut stack = Stack::new();
    let _map_s = KeyStack::new();
    let guard = _map_s.guard();
    let count = stack.push(v);
    let result =  _map_s.try_insert(k, stack, &guard);
    match result {
        Ok(Stack) => return ReturnValue::Ok,
        Err(TryInsertError) => return ReturnValue::Nil,

    }
    //return ReturnValue::Nil;
}

fn stpop(k: Key) -> ReturnValue {
    let _map = KeyStack::new();
    let guard = _map.guard();
    let ss = _map.get(&k, &guard);
    //return ss.and_then(|mut st| st.pop().as_deref()).map(ReturnValue::StringRes).unwrap_or(ReturnValue::Nil);
    return ReturnValue::Nil;
}

fn stpeek(k: Key) -> ReturnValue {

    return ReturnValue::Nil;
}

fn stsize(k: Key) -> ReturnValue {

    return ReturnValue::Nil;
}

