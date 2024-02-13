use std::error::Error;

use crate::commands::event::Event;
use rusqlite::{params, Connection, Result};

const CREATE_EVENTS: &str = "CREATE TABLE IF NOT EXISTS events (
    id INTEGER NOT NULL PRIMARY KEY,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    event_type VARCHAR(32) NOT NULL
);";

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn new() -> Result<Db, Box<dyn Error>> {
        let conn: Connection = Connection::open("wflow.db")?;
        conn.execute(&CREATE_EVENTS, [])?;

        Ok(Db { conn })
    }

    pub fn insert_event(&mut self, event: &Event) -> Result<()> {
        let event_type: String = event.event_type.to_string();
        self.conn.execute(
            "INSERT INTO events (timestamp, event_type) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'), ?)",
            params![event_type],
        )?;

        Ok(())
    }
}
