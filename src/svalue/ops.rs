use bytes::Bytes;
use std::convert::TryFrom;
use std::fmt::Debug;

use super::stack::StackOps;
use super::types::{ReturnValue};

#[derive(Debug, Clone)]
pub enum Ops {
    Stacks(StackOps),
}