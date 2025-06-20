use crate::db::db::Db;
use chrono::{DateTime, Local, NaiveDate};
use parking_lot::Mutex;
use rusqlite::{Connection, Result};
use std::error::Error;
use std::sync::Arc;

const SCHEMA_BREAKS: &str = "CREATE TABLE IF NOT EXISTS breaks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    start TEXT NOT NULL,
    end TEXT,
    duration INTEGER
)";
const INSERT_BREAK: &str = "INSERT INTO breaks (start) VALUES (?1)";
const UPDATE_BREAK: &str = "UPDATE breaks SET end = ?1, duration = ?2 WHERE id = ?3";
const SELECT_LAST_BREAK: &str = "SELECT id, start FROM breaks WHERE end IS NULL ORDER BY id DESC LIMIT 1";
const SELECT_DAILY_BREAKS: &str = "SELECT id, start, end, duration FROM breaks WHERE date(start) = ?1 AND (duration IS NULL OR duration >= ?2)";

#[derive(Debug)]
pub struct Break {
    pub id: i64,
    pub start: String,         // RFC3339 timestamp
    pub end: Option<String>,   // RFC3339 timestamp
    pub duration: Option<i64>, // Duration in seconds
}

// Manages operations for the 'breaks' table.
pub struct Breaks {
    conn: Arc<Mutex<Connection>>,
}

impl Breaks {
    // Creates a new Breaks instance and initializes the 'breaks' table.
    pub fn new() -> Result<Breaks, Box<dyn Error>> {
        let db_conn = Db::new()?.conn;
        db_conn.execute(&SCHEMA_BREAKS, [])?;
        Ok(Breaks {
            conn: Arc::new(Mutex::new(db_conn)),
        })
    }

    // Inserts a new break start record with the current timestamp.
    pub fn insert_start(&self) -> Result<()> {
        let start = chrono::Local::now().to_rfc3339();
        let conn_guard = self.conn.lock();
        conn_guard.execute(&INSERT_BREAK, [&start])?;
        Ok(())
    }

    // Updates the most recent open break (end IS NULL) with an end timestamp and duration.
    pub fn insert_end(&self) -> Result<(), Box<dyn Error>> {
        let end = chrono::Local::now();
        let end_str = end.to_rfc3339();

        let conn_guard = self.conn.lock();

        let mut stmt = conn_guard.prepare(&SELECT_LAST_BREAK)?;
        let break_row = stmt.query_row([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)));

        if let Ok((id, start_str)) = break_row {
            let start = DateTime::parse_from_rfc3339(&start_str)?.with_timezone(&Local);
            let duration = (end - start).num_seconds();
            conn_guard.execute(&UPDATE_BREAK, [&end_str, &duration.to_string(), &id.to_string()])?;
        }
        Ok(())
    }

    // Fetches breaks for a given date, filtering by minimum duration (in minutes).
    pub fn fetch(&self, date: NaiveDate, min_duration: u64) -> Result<Vec<Break>, Box<dyn Error>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let min_duration_secs = (min_duration * 60) as i64; // Convert minutes to seconds

        let conn_guard = self.conn.lock();
        let mut stmt = conn_guard.prepare(&SELECT_DAILY_BREAKS)?;

        let break_iter = stmt.query_map([&date_str, &min_duration_secs.to_string()], |row| {
            Ok(Break {
                id: row.get(0)?,
                start: row.get(1)?,
                end: row.get(2)?,
                duration: row.get(3)?,
            })
        })?;

        let mut breaks = Vec::new();
        for break_result in break_iter {
            breaks.push(break_result?);
        }
        Ok(breaks)
    }
}
