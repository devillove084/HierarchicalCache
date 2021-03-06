use std::{hash::Hasher, marker::PhantomData, ops::DerefMut, rc::Rc, string, time::{Duration, SystemTime}};

use cache::{Cache, OnEvict};
use lazy_static::__Deref;
use rand::Rng;

use crate::{lcache::{cache::{self, VoidEvict}, tiny_lfu::MAX_WINDOW_SIZE}, svalue::{dict::Dict, object::{Robj, RobjPointer}}};
use crate::svalue::object::RobjPtr;
use crate::svalue::hash::string_object_hash;

pub struct DBCache {
    id: usize,
    store: Cache<RobjPointer, RobjPointer>,
    expires: Cache<RobjPointer, SystemTime>,
}

impl DBCache {
    pub fn new(id: usize) -> DBCache {
        DBCache {
            id,
            store: Cache::with_window_size(1024 * 1024, 100000),
            expires: Cache::with_window_size(1024 * 1024, 100000),
        }
    }

    pub fn insert(&mut self, key: RobjPointer, value: RobjPointer) -> Result<Option<RobjPointer>, Option<()>> {
        self.store.insert(key, value)
    }

    pub fn remove_expire(&mut self, key: &RobjPointer) -> Result<(), ()> {
        let _  = self.expires.remove(&key).ok_or_else(|| 0);
        Ok(())
    }

    pub fn set_expire(&mut self, key: RobjPointer, when: SystemTime) -> Result<Option<SystemTime>, Option<()>> {
        self.expires.insert(key, when)
    }

    pub fn get_expire(&mut self, key: &RobjPointer) -> Option<&SystemTime> {
        self.expires.get_mut(key)
    }

    pub fn expire_if_needed(&mut self, key: &RobjPointer) -> Result<bool, ()> {
        Ok(true)
    }

    pub fn delete(&mut self, key: &RobjPointer) -> Result<(), ()> {
        if self.expires.len() == 0 {
            return Err(())
        }

        let _ = self.expires.remove(key).ok_or_else(||0);
        let _ = self.store.remove(key).ok_or_else(|| 0);
        Ok(())
    }

    pub fn delete_key(&mut self, key: &RobjPointer) -> Result<(), ()> {
        if self.expires.len() != 0 {
            let _ = self.expires.remove(key);
        }
        self.store.remove(key).ok_or_else(|| 0);
        Ok(())
    }

    pub fn look_up_key_read(&mut self, key: &RobjPointer) -> Option<&RobjPointer> {
        let _ = self.expire_if_needed(key);
        self.look_up_key(key)
    }

    pub fn look_up_key(&mut self, key: &RobjPointer) -> Option<&RobjPointer> {
        let e = self.store.get_mut(key);
        let res = Some(e.unwrap());
        res
        // match e {
        //     None => None,
        //     Some(&mut RobjPointer) => Some(RobjPointer),
        // }
    }
}


pub struct DB {
    pub id: usize,
    pub dict: Dict<RobjPtr, RobjPtr>,
    pub expires: Dict<RobjPtr, SystemTime>,
}

impl DB {
    pub fn new(id: usize) -> DB {
        let mut rng = rand::thread_rng();
        DB {
            id,
            dict: Dict::new(string_object_hash, rng.gen()),
            expires: Dict::new(string_object_hash, rng.gen()),
        }
    }

    pub fn remove_expire(&mut self, key: &RobjPtr) -> Result<(), ()> {
        let _ = self.expires.delete(key)?;
        Ok(())
    }

    pub fn set_expire(&mut self, key: RobjPtr, when: SystemTime) -> Result<(), ()> {
        self.expires.add(key, when)
    }

    pub fn get_expire(&mut self, key: &RobjPtr) -> Option<&SystemTime> {
        self.expires.find_by_mut(key).map(|p| p.1)
    }

    pub fn expire_if_needed(&mut self, key: &RobjPtr) -> Result<bool, ()> {
        if self.expires.len() == 0 {
            return Err(());
        }
        
        let r = self.expires.find(key);
        if r.is_none() {
            return Err(())
        }

        let when = r.unwrap().1;
        if SystemTime::now() < *when {
            return Ok(false);
        }
        self.expires.delete(key).unwrap();

        let _ = self.dict.delete(key)?;
        Ok(true)
    }

    pub fn delete(&mut self, key: &RobjPtr) -> Result<(), ()> {
        if self.expires.len() == 0 {
            return Err(())
        }

        let _ = self.expires.delete(key)?;
        let _ = self.dict.delete(key)?;
        Ok(())
    }

    pub fn delete_key(&mut self, key: &RobjPtr) -> Result<(), ()> {
        if self.expires.len() != 0 {
            let _ = self.expires.delete(key);
        }
        self.dict.delete(key)?;
        Ok(())
    }

    pub fn look_up_key_read(&mut self, key: &RobjPtr) -> Option<RobjPtr> {
        let _ = self.expire_if_needed(key);
        self.look_up_key(key)
    }

    pub fn look_up_key(&mut self, key: &RobjPtr) -> Option<RobjPtr> {
        let e = self.dict.find_by_mut(key);
        match e {
            None => None,
            Some((_, r)) => Some(Rc::clone(r)),
        }
    }

}