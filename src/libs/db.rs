use rusqlite::{Connection, Result};

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn new() -> Result<Self> {
        let conn: Connection = Connection::open("wflow.db")?;

        Ok(Db { conn })
    }
}
