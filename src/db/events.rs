use super::db::Db;
use crate::libs::event::EventType;
use rusqlite::{params, Connection, OptionalExtension, Result};
use std::error::Error;

const SCHEMA_EVENTS: &str = "CREATE TABLE IF NOT EXISTS events (
    id INTEGER NOT NULL PRIMARY KEY,
    start TIMESTAMP NOT NULL,
    end TIMESTAMP
);";
const INSERT_EVENT: &str = "INSERT INTO events (start) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'))";
const SELECT_LAST_EVENT: &str = "SELECT id, end FROM events ORDER BY id DESC LIMIT 1";
const UPDATE_EVENT: &str = "UPDATE events SET end = datetime(CURRENT_TIMESTAMP, 'localtime') WHERE id = ?1";

pub struct Events {
    pub conn: Connection,
}

impl Events {
    pub fn new() -> Result<Events, Box<dyn Error>> {
        let db = Db::new()?;
        db.conn.execute(&SCHEMA_EVENTS, [])?;

        Ok(Events { conn: db.conn })
    }

    pub fn insert(&mut self, event_type: &EventType) -> Result<()> {
        let _ = match event_type {
            EventType::Start => self.start(),
            EventType::End => self.end(),
        };

        Ok(())
    }

    fn start(&mut self) -> Result<()> {
        self.conn.execute(INSERT_EVENT, [])?;

        Ok(())
    }

    fn end(&mut self) -> Result<()> {
        let transaction = self.conn.transaction()?;

        let maybe_row = transaction
            .query_row(SELECT_LAST_EVENT, [], |row| Ok((row.get::<_, i32>(0)?, row.get::<_, Option<String>>(1)?)))
            .optional()?;

        if let Some((id, end)) = maybe_row {
            println!("{}   {:?}", id, end);
            if end.is_none() {
                transaction.execute(UPDATE_EVENT, params![id])?;
                transaction.commit()?;
                return Ok(());
            }
        }

        Ok(())
    }
}
