#![allow(non_snake_case)]
#![allow(unused_doc_comments)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(non_camel_case_types)]
#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_must_use)]

//use std::sync::Arc;

#[macro_use]
extern crate lazy_static;

mod gossip;
mod chashmap;
mod lcache;
mod svalue;
mod db;
mod crdts;

use std::string;

use db::db::DBCache;
// use db::*;
use svalue::{object::{Robj, RobjPtr}, zip_list::ZipList};
//use svalue::{self::*, zip_list::{self, ZipListNodeMut}};
//use zip_list::ZipList;

use lcache::{Cache, OnEvict};
use rand::Rng;
// use std::time::Duration;

// #[derive(Default, Debug)]
// struct Evict {}

// impl OnEvict<usize, usize> for Evict {
//     fn evict(&self, k: &usize, v: &usize) {
//         println!("Evict item.  k={}, v={}", k, v);
//     }
// }

// #[derive(Default, Debug)]
// struct my_struct {}

// impl OnEvict<usize, ZipList> for my_struct {
//     fn evict(&self, k: &usize, v: &ZipList) {
//         println!("Evict item. k = {}", k);
//     }
// }


// #[derive(Default,Debug)]
// struct DB_Cache {}

// impl OnEvict<usize, db::db::DB> for DB_Cache {
//     fn evict(&self, k: &usize, v: &db::db::DB) {
//         println!("hhhhh {}", k);
//     }
// }

fn main() {
    
    // let mut test = Cache::with_on_evict(100000, DB_Cache::default()).with_metrics();
    // let mut db_with_test = db::db::DB::new(0);
    
    // for i in 0..100 {
    //     db_with_test.dict.add(Robj::create_string_object_from_long(i), Robj::create_string_object_from_long(i));
    // }

    let mut key = vec![1u8];
    let mut value = vec![1u8];
    let mut r = rand::thread_rng();
    for i in 0..999 {
        let n: u8 = r.gen();
        key.push(n);
        value.push(n);
    }

    println!("This is len of key {}", key.len());
    println!("This is len of value {}", value.len());

    let mut db_test = DBCache::new(0);




    
    // let mut bug = db::db::DBCache::new(0);
    // let mut l = ZipList::new();
    // let content = &['a' as u8, 100];
    // l.push(content);
    // let v_test = String::from("Value");
    // //bug.insert(v_test, l);
    // let mut test_1 = Robj::cre
    
    
    
    // let mut l = zip_list::ZipList::new();
    // let content = &['a' as u8; 1000];
    // let content_2 = &['b' as u8; 1000];
    // //println!("{:?}", content);

    // //let mut c = db::db::DB::new();
    

    // l.push(content);
    // l.push(content_2);
    // let result = test.insert(1, l).expect("Item is not inserted");
    // let p = test.get(&1).unwrap();
    

    //db::server::ttest();
    //let _ = db::db::DB::new(0);
    // let mut list = ZipList::new();
    // let content = &['a' as u8; 250];
    // list.push(content);
    // list.push(content);
    // list.push("11".as_bytes());
    // //list.inner_insert(list.header_len(), big);
    // let result = list.len();
    // println!("Len is {}", result);

    // This is for cache test!!!
    // let mut cache = Cache::with_on_evict(10, Evict::default()).with_metrics();
    // assert!(cache.is_empty());
    // assert_eq!(cache.get(&1), None);
    // cache.insert(1, 1).expect("Item is not inserted");
    // assert_eq!(cache.get(&1), Some(&1));
    // let previous = cache.insert(1, 2).expect("Item is not updated");
    // assert_eq!(previous, Some(1));
    // assert_eq!(cache.get(&1), Some(&2));
    // cache
    //     .insert_with_ttl(2, 2, Duration::from_secs(1))
    //     .expect("Item is not inserted");
    // assert!(cache.contains(&2));
    // std::thread::sleep(Duration::from_secs(2));
    // assert!(!cache.contains(&2));
    // {
    //     let v = cache.get_mut(&1).unwrap();
    //     *v = 3;
    // }
    // assert_eq!(cache.get(&1), Some(&3));
    // for i in 0..25 {
    //     match cache.insert(i, i) {
    //         Ok(_) => println!("Item is inserted. i: {}", i),
    //         Err(_) => println!("Item is rejected. i: {}", i),
    //     }
    // }
    // for (k, v) in cache.iter() {
    //     println!("Item: k: {}, v: {}", k, v);
    // }
    // println!(
    //     "\nCache metrics. {:?}",
    //     cache.metrics().expect("Cache should have metrics")
    // );

    // let _map = HashMap::<usize, usize>::new();

    // let guard = _map.guard();
    // let old = _map.insert(42, 0, &guard);
    // assert!(old.is_none());
    // let e = _map.get_key_value(&42, &guard);
    // print!("{:?}", e);

    // print!("Single insert and query!!");

    // let map = Arc::new(HashMap::<usize, usize>::new());

    // let map1 = map.clone();
    // let t1 = std::thread::spawn(move || {
    //     for i in 0..4 {
    //         map1.insert(i, 0, &map1.guard());
    //         println!("Insert1 Done!");
    //     }
    // });
    // let map2 = map.clone();
    // let t2 = std::thread::spawn(move || {
    //     for i in 0..4 {
    //         map2.insert(i, 1, &map2.guard());
    //         println!("Insert2 Done!");
    //     }
    // });

    // t1.join().unwrap();
    // t2.join().unwrap();

    // let guard = map.guard();
    // for i in 0..4 {
    //     let v = map.get(&i, &guard).unwrap();
    //     println!("{:?}", v);
    //     assert!(v == &0 || v == &1);

    //     let kv = map.get_key_value(&i, &guard).unwrap();
    //     println!("{:?}", kv);
    //     assert!(kv == (&i, &0) || kv == (&i, &1));
    // }
}
