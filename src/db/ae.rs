use core::panic;
use std::{cell::RefCell, collections::VecDeque, rc::Rc, time::SystemTime};

use mio::*;
use mio::net::{TcpListener, TcpStream};
use crate::chashmap::HashMap;
use super::client::ClientData;

use super::server::Server;

pub type Fd = Rc<RefCell<Fdp>>;

pub enum Fdp {
    Listener(TcpListener),
    Stream(TcpStream),
    Nil,
}

impl Fdp {
    pub fn is_listener(&self) -> bool {
        match self {
            Fdp::Listener(_) => true,
            _ => false,
        }
    }

    pub fn is_stream(&self) -> bool {
        match self {
            Fdp::Stream(_) => true,
            _ => false,
        }
    }

    // pub fn to_evented(&self) -> &dyn Evented {
    //     match self {
    //         Fdp::Stream(s) => s,
    //         Fdp::Listener(l) => l,
    //         _ => panic!("cannot make Nil to evented"),
    //     }
    // }

    pub fn unwrap_listener(&self) -> &TcpListener {
        match self {
            Fdp::Listener(l) => l,
            _ => panic!("not a listener"),
        }
    }

    pub fn unwrap_stream(&self) -> &TcpStream {
        match self {
            Fdp::Stream(s) => s,
            _ => panic!("not a stream"),
        }
    }

    pub fn unwrap_stream_mut(&mut self) -> &mut TcpStream {
        match self {
            Fdp::Stream(s) => s,
            _ => panic!("not a stream"),
        }
    }
}

type AeTimeProc = fn(server: Server, el: &mut AeEventLoop, id: i64, data: &ClientData) -> i32;
type AeFileProc = fn(server: Server, el: &mut AeEventLoop, fd: &Fd, data: &ClientData, mask: i32);
type AeEventFinalizerProc = fn(el: &mut AeEventLoop, data: &ClientData);

fn _default_ae_time_proc(_server: &mut Server, _el: &mut AeEventLoop, _id: i64,
    _data: &ClientData) -> i32 { 1 }

pub fn default_ae_file_proc(_server: &mut Server, _el: &mut AeEventLoop,
       _fd: &Fd, _data: &ClientData, _mask: i32) {
        panic!("Default file proc should never be called");
}

pub fn default_ae_event_finalizer_proc(_el: &mut AeEventLoop, _data: &ClientData) {}


// struct AeFileEvent {
//     fd: Fd,
//     mask: i32,
//     r_file_proc: AeFileProc,
//     w_file_proc: AeFileProc,
//     finalizer_proc: AeEventFinalizerProc,
//     client_data: ClientData,
// }

// impl AeFileEvent {
//     fn new(fd: Fd) -> AeFileEvent {
//         AeFileEvent {
//             fd,
//             mask: 0,
//             r_file_proc: default_ae_file_proc,
//             w_file_proc: default_ae_file_proc,
//             finalizer_proc: default_ae_event_finalizer_proc,
//             client_data: ClientData::Nil(),
//         }
//     }
// }

struct AeTimeEvent {
    id: i64,
    when: SystemTime,
    time_proc: AeTimeProc,
    finalizer_proc: AeEventFinalizerProc,
    client_data: ClientData,
}

enum ActiveReturnAction {
    None,
    Delete,
    Merge(i32, AeFileProc),
    Reduce(i32),
}

pub struct AeEventLoop {
    time_event_next_id: i64,
    //file_events_hash: HashMap<Token, AeFileEvent>,
    file_events_num: usize,
    active_return_action: ActiveReturnAction,
    time_events: VecDeque<AeTimeEvent>,
    poll: Poll,
    stop: bool,
}