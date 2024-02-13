use super::db::Db;
use crate::libs::event::Event;
use rusqlite::{params, Connection, Result};
use std::error::Error;

const SCHEMA_EVENTS: &str = "CREATE TABLE IF NOT EXISTS events (
    id INTEGER NOT NULL PRIMARY KEY,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    event_type VARCHAR(32) NOT NULL
);";
const INSERT_EVENT: &str = "INSERT INTO events (timestamp, event_type) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'), ?)";

pub struct Events {
    pub conn: Connection,
}

impl Events {
    pub fn new() -> Result<Events, Box<dyn Error>> {
        let db = Db::new()?;
        db.conn.execute(&SCHEMA_EVENTS, [])?;

        Ok(Events { conn: db.conn })
    }

    pub fn insert(&mut self, event: &Event) -> Result<()> {
        let event_type: String = event.event_type.to_string();
        self.conn.execute(INSERT_EVENT, params![event_type])?;

        Ok(())
    }
}
