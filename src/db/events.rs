use super::db::Db;
use crate::libs::event::{Event, EventType};
use chrono::{
    prelude::{Local, NaiveDateTime, Timelike},
    NaiveDate,
};
use rusqlite::{params, Connection, OptionalExtension, Result};
use std::{collections::HashMap, error::Error};

const SCHEMA_EVENTS: &str = "CREATE TABLE IF NOT EXISTS events (
    id INTEGER NOT NULL PRIMARY KEY,
    start TIMESTAMP NOT NULL,
    end TIMESTAMP
);";
const INSERT_EVENT: &str = "INSERT INTO events (start) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'))";
const SELECT_LAST_EVENT: &str = "SELECT id, end FROM events ORDER BY id DESC LIMIT 1";
const UPDATE_EVENT: &str = "UPDATE events SET end = datetime(CURRENT_TIMESTAMP, 'localtime') WHERE id = ?1";
const SELECT_EVENTS: &str = "SELECT id, start, end FROM events WHERE date(start) = date('now', 'localtime') ORDER BY start";
const SELECT_MONTHLY_EVENTS: &str = "SELECT id, start, end FROM events WHERE strftime('%Y-%m', start) = strftime('%Y-%m', 'now')";

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

    pub fn fetch(&mut self) -> Result<Vec<Event>, Box<dyn Error>> {
        let mut stmt = self.conn.prepare(SELECT_EVENTS)?;
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

    pub fn event_summary(&mut self) -> Result<HashMap<NaiveDate, Vec<Event>>, Box<dyn Error>> {
        let mut events: HashMap<NaiveDate, Vec<Event>> = HashMap::new();
        let now = Local::now();
        let mut stmt = self.conn.prepare(SELECT_MONTHLY_EVENTS)?;

        let event_iter = stmt.query_map([], |row| {
            let end: Option<NaiveDateTime> = row.get(2)?;
            Ok(Event {
                id: row.get(0)?,
                start: row.get(1)?,
                end: Some(end.unwrap_or(now.naive_local().with_nanosecond(0).unwrap())),
                duration: None,
            })
        })?;

        for event_result in event_iter {
            let event = event_result?;
            let event_date = event.start.date();
            events.entry(event_date).or_insert_with(Vec::new).push(event);
        }

        Ok(events)
    }

    pub fn _event_summary(&self) -> Result<EventSummary, Box<dyn Error>> {
        let mut event_summary = EventSummary {
            daily_hours: vec![],
            total_hours: 0.0,
        };
        let mut daily_hours = HashMap::new();
        let mut stmt = self.conn.prepare(SELECT_MONTHLY_EVENTS)?;

        let events_iter = stmt.query_map([], |row| {
            let start: NaiveDateTime = row.get(0)?;
            let end: NaiveDateTime = row.get(1)?;
            Ok((start, end))
        })?;

        for event in events_iter {
            let (start, end) = event?;
            let duration = end.signed_duration_since(start);
            let hours = duration.num_minutes() as f64 / 60.0;
            let date = start.date();

            *daily_hours.entry(date).or_insert(0.0) += hours;
        }

        for (date, hours) in daily_hours {
            event_summary.daily_hours.push((date, hours));
            event_summary.total_hours += hours;
        }

        Ok(event_summary)
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
