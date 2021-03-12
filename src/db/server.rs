
use std::net::SocketAddr;

use crate::crdts;

use super::db::DB;

pub struct Server {
    //port: u16,
    db: Vec<DB>,
    local_addr: SocketAddr,
}

