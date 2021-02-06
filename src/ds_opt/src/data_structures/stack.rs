use crate::types::Count;
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