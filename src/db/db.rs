use crate::libs::data_storage::DataStorage;
use rusqlite::{Connection, Result};
use std::error::Error;

pub const DB_FILE_NAME: &str = "kasl.db";
pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn new() -> Result<Db, Box<dyn Error>> {
        let db_file_path = DataStorage::new().get_path(DB_FILE_NAME)?;
        let conn: Connection = Connection::open(db_file_path)?;

        Ok(Db { conn })
    }
}
