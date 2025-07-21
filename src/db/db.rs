use crate::libs::data_storage::DataStorage;
use anyhow::Result;
use rusqlite::Connection;

pub const DB_FILE_NAME: &str = "kasl.db";
pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn new() -> Result<Db> {
        let db_file_path = DataStorage::new().get_path(DB_FILE_NAME)?;
        let conn: Connection = Connection::open(db_file_path)?;

        Ok(Db { conn })
    }
}
