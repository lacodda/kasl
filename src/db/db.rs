use crate::db::migrations;
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
        let mut conn: Connection = Connection::open(db_file_path)?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        // Run migrations
        migrations::init_with_migrations(&mut conn)?;

        Ok(Db { conn })
    }

    /// Get a new connection without running migrations (for migration commands)
    pub fn new_without_migrations() -> Result<Connection> {
        let db_file_path = DataStorage::new().get_path(DB_FILE_NAME)?;
        let conn = Connection::open(db_file_path)?;
        Ok(conn)
    }
}
