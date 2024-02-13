use rusqlite::{Connection, Result};
use std::error::Error;

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn new() -> Result<Db, Box<dyn Error>> {
        let conn: Connection = Connection::open("wflow.db")?;

        Ok(Db { conn })
    }
}
