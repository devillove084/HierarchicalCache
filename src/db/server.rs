use super::db::DB;
use super::ae::Fd;

pub struct Server {
    pub port: u16,
    pub db: Vec<DB>,
    pub fd: Fd,
}
