#![feature(test, async_closure)]
#![feature(const_fn)]

#[macro_use]
extern crate serde_derive;

// #[macro_use]
// extern crate lazy_static;

// //extern crate slog;

// extern crate rmp_serde as rmps;

// pub mod hashes;
// pub mod keys;
// pub mod lists;

#[macro_use]
pub mod macros;
pub mod data_structures;
pub mod ops;
// pub mod sets;
// pub mod sorted_sets;
// pub mod stack;
pub mod types;
// pub mod bloom;
//pub mod misc;
pub mod state;