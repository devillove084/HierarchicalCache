use super::db::DB;

pub struct Server {
    pub port: u16,
    pub db: Vec<DB>,  //TODO: change to hashmap
}


