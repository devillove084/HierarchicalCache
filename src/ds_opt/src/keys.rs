use crate::op_variants;
use crate::ops::RVec;
use crate::types::{Count, Key, ReturnValue, StateRef, Value};

op_variants! {
    KeyOps,
    Set(Key, Value),
    MSet(RVec<(Key, Value)>),
    Get(Key),
    MGet(RVec<Key>),
    Del(RVec<Key>),
    Rename(Key, Key),
    RenameNx(Key, Key),
    Test(Key)
}

pub async fn key_interact(key_op: KeyOps, state: StateRef) -> ReturnValue {
    match key_op {
        KeyOps::Get(key) => state.kv.get(&key).map_or(ReturnValue::Nil, |v| {
            ReturnValue::StringRes(v.value().clone())
        }),
        KeyOps::MGet(keys) => {
            let vals = keys
                .iter()
                .map(|key| match state.kv.get(key) {
                    Some(v) => ReturnValue::StringRes(v.value().clone()),
                    None => ReturnValue::Nil,
                })
                .collect();
            ReturnValue::Array(vals)
        }
        KeyOps::Set(key, value) => {
            state.kv.insert(key, value);
            ReturnValue::Ok
        }
        KeyOps::MSet(key_vals) => {
            let kv = &state.kv;
            for (key, val) in key_vals.into_iter() {
                kv.insert(key, val);
            }
            ReturnValue::Ok
        }
        KeyOps::Del(keys) => {
            let deleted = keys
                .iter()
                .map(|x| state.kv.remove(x))
                .filter(Option::is_some)
                .count();
            ReturnValue::IntRes(deleted as Count)
        }
        KeyOps::Rename(key, new_key) => match state.kv.remove(&key) {
            Some((_, value)) => {
                state.kv.insert(new_key, value);
                ReturnValue::Ok
            }
            None => ReturnValue::Error(b"no such key"),
        },
        KeyOps::RenameNx(key, new_key) => {
            if state.kv.contains_key(&new_key) {
                return ReturnValue::IntRes(0);
            }
            match state.kv.remove(&key) {
                Some((_, value)) => {
                    state.kv.insert(new_key, value);
                    ReturnValue::IntRes(1)
                }
                None => ReturnValue::Error(b"no such key"),
            }
        }
        // TODO: Keep this?
        KeyOps::Test(_key) => {
            ReturnValue::Ok
        }
    }
}