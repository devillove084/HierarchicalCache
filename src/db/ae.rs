use core::panic;
use std::{cell::RefCell, rc::Rc};

use mio::*;
use mio::net::{TcpListener, TcpStream};
use super::server::Server;

// type AeTimeProc = fn(server: &mut Server, el: &mut AeEventLoop, id: i64, data: &ClientData) -> i32;
// type AeFileProc = fn(server: &mut Server, el: &mut AeEventLoop, fd: &Fd, data: &ClientData, mask: i32);
// type AeEventFinalizerProc = fn(el: &mut AeEventLoop, data: &ClientData);
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