#![allow(non_snake_case)]

use std::sync::Arc;
use chash_map::*;

fn main() {
    
    let _map = HashMap::<usize, usize>::new();

    let guard = _map.guard();
    let old = _map.insert(42, 0, &guard);
    assert!(old.is_none());
    let e = _map.get_key_value(&42, &guard);
    print!("{:?}", e);


    print!("Single insert and query!!");

    let map = Arc::new(HashMap::<usize, usize>::new());

    let map1 = map.clone();
    let t1 = std::thread::spawn(move || {
        for i in 0..4 {
            map1.insert(i, 0, &map1.guard());
            println!("Insert1 Done!");
        }
    });
    let map2 = map.clone();
    let t2 = std::thread::spawn(move || {
        for i in 0..4 {
            map2.insert(i, 1, &map2.guard());
            println!("Insert2 Done!");
        }
    });

    t1.join().unwrap();
    t2.join().unwrap();

    let guard = map.guard();
    for i in 0..4 {
        let v = map.get(&i, &guard).unwrap();
        println!("{:?}", v);
        assert!(v == &0 || v == &1);

        let kv = map.get_key_value(&i, &guard).unwrap();
        println!("{:?}", kv);
        assert!(kv == (&i, &0) || kv == (&i, &1));
    }

}