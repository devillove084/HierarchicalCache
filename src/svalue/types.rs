use std::collections::VecDeque;

use bytes::Bytes;
use smallvec::SmallVec;

use crate::chashmap::HashMap;

use super::stack::Stack;
// use std::collections::{HashMap, HashSet, VecDeque};
// use std::convert::From;
// use std::sync::atomic::AtomicU64;
// use std::sync::Arc;

pub type Value = Bytes;
pub type Key = Bytes;
pub type Count = i64;
pub type Index = i64;
pub type Score = i64;
pub type UTimeout = i64;
pub type RedisBool = i64;

//pub type DumpFile = Arc<Mutex<File>>;

/// ValueRef is the canonical type for values flowing
/// through the system. Inputs are converted into Values,
/// and outputs are converted into Values.
#[derive(PartialEq, Clone)]
pub enum ValueRef {
    BulkString(Bytes),
    SimpleString(Bytes),
    Error(Bytes),
    ErrorMsg(Vec<u8>),
    Int(i64),
    Array(Vec<ValueRef>),
    NullArray,
    NullBulkString,
}

impl std::fmt::Debug for ValueRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueRef::BulkString(s) => write!(
                f,
                "ValueRef::BulkString({:?})",
                String::from_utf8_lossy(s)
            ),
            ValueRef::SimpleString(s) => write!(
                f,
                "ValueRef::SimpleString({:?})",
                String::from_utf8_lossy(s)
            ),
            ValueRef::Error(s) => {
                write!(f, "ValueRef::Error({:?})", String::from_utf8_lossy(s))
            }
            ValueRef::ErrorMsg(s) => write!(f, "ValueRef::ErrorMsg({:?})", s),

            ValueRef::Int(i) => write!(f, "ValueRef::Int({:?})", i),
            ValueRef::NullBulkString => write!(f, "ValueRef::NullBulkString"),
            ValueRef::NullArray => write!(f, "ValueRef::NullArray"),
            ValueRef::Array(arr) => {
                write!(f, "ValueRef::Array(")?;
                for item in arr {
                    write!(f, "{:?}", item)?;
                    write!(f, ",")?;
                }
                write!(f, ")")?;
                Ok(())
            }
        }
    }
}

const DEFAULT_SMALL_VEC_SIZE: usize = 2;
pub type RVec<T> = SmallVec<[T; DEFAULT_SMALL_VEC_SIZE]>;


pub enum ReturnValue {
    Ok,
    StringRes(Value),
    Error(&'static [u8]),
    MultiStringRes(Vec<Value>),
    Array(Vec<ReturnValue>),
    IntRes(i64),
    Nil,
    Ident(ValueRef),
}

impl From<Count> for ReturnValue {
    fn from(int: Count) -> ReturnValue {
        ReturnValue::IntRes(int)
    }
}

impl From<RVec<Value>> for ReturnValue {
    fn from(vals: RVec<Value>) -> ReturnValue {
        ReturnValue::Array(vals.into_iter().map(ReturnValue::StringRes).collect())
    }
}

impl From<Vec<String>> for ReturnValue {
    fn from(strings: Vec<String>) -> ReturnValue {
        let strings_to_bytes: Vec<Bytes> = strings
            .into_iter()
            .map(|s| s.as_bytes().to_vec().into())
            .collect();
        ReturnValue::MultiStringRes(strings_to_bytes)
    }
}

impl ReturnValue {
    pub fn is_error(&self) -> bool {
        if let ReturnValue::Error(_) = *self {
            return true;
        }
        false
    }
}

pub type KeyList = HashMap<Key, VecDeque<Value>>;
pub type KeyStack = HashMap<Key, Stack<Value>>;
