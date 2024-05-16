use super::db::Db;
use crate::libs::event::{Event, EventType};
use chrono::NaiveDate;
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
const SELECT_DAILY_EVENTS: &str = "SELECT id, start, end FROM events WHERE date(start) = date('now', 'localtime') ORDER BY start";
const SELECT_MONTHLY_EVENTS: &str = "SELECT id, start, end FROM events WHERE strftime('%Y-%m', start) = strftime('%Y-%m', 'now')";

pub enum SelectRequest {
    Daily,
    Monthly,
}

impl SelectRequest {
    fn value(&self) -> &str {
        match *self {
            Self::Daily => SELECT_DAILY_EVENTS,
            Self::Monthly => SELECT_MONTHLY_EVENTS,
        }
    }
}

#[derive(Debug)]
pub struct EventSummary {
    pub daily_hours: Vec<(NaiveDate, f64)>,
    pub total_hours: f64,
}

#[derive(Debug)]
pub struct Events {
    pub conn: Connection,
}

impl Events {
    pub fn new() -> Result<Events, Box<dyn Error>> {
        let db = Db::new()?;
        db.conn.execute(&SCHEMA_EVENTS, [])?;

        Ok(Events { conn: db.conn })
    }

    pub fn fetch(&mut self, select_request: SelectRequest) -> Result<Vec<Event>, Box<dyn Error>> {
        let mut stmt = self.conn.prepare(select_request.value())?;
        let event_iter = stmt.query_map([], |row| {
            Ok(Event {
                id: row.get(0)?,
                start: row.get(1)?,
                end: row.get(2)?,
                duration: None,
            })
        })?;

        let mut events = vec![];
        for event in event_iter {
            events.push(event?);
        }

        Ok(events)
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
            if end.is_none() {
                transaction.execute(UPDATE_EVENT, params![id])?;
                transaction.commit()?;
                return Ok(());
            }
        }

        Ok(())
    }
}
