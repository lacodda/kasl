use crate::db::db::Db;
use crate::libs::pause::Pause;
use chrono::{Local, NaiveDate, NaiveDateTime, TimeDelta};
use parking_lot::Mutex;
use rusqlite::{Connection, Result};
use std::error::Error;
use std::sync::Arc;

const SCHEMA_PAUSES: &str = "CREATE TABLE IF NOT EXISTS pauses (
    id INTEGER NOT NULL PRIMARY KEY,
    start TIMESTAMP NOT NULL,
    end TIMESTAMP,
    duration INTEGER
)";
const INSERT_PAUSE: &str = "INSERT INTO pauses (start) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'))";
const INSERT_PAUSE_WITH_TIME: &str = "INSERT INTO pauses (start) VALUES (?1)";
const UPDATE_PAUSE: &str = "UPDATE pauses SET end = (datetime(CURRENT_TIMESTAMP, 'localtime')), duration = ?1 WHERE id = ?2";
const SELECT_LAST_PAUSE: &str = "SELECT id, start FROM pauses WHERE end IS NULL ORDER BY id DESC LIMIT 1";
const SELECT_DAILY_PAUSES: &str =
    "SELECT id, start, end, duration FROM pauses WHERE date(start) = date(?1, 'localtime') AND (duration IS NULL OR duration >= ?2)";

// Manages operations for the 'pauses' table.
pub struct Pauses {
    pub conn: Arc<Mutex<Connection>>,
}

impl Pauses {
    // Creates a new Pauses instance and initializes the 'pauses' table.
    pub fn new() -> Result<Pauses, Box<dyn Error>> {
        let db_conn = Db::new()?.conn;
        db_conn.execute(SCHEMA_PAUSES, [])?;
        Ok(Pauses {
            conn: Arc::new(Mutex::new(db_conn)),
        })
    }

    // Inserts a new pause start record with the current timestamp.
    pub fn insert_start(&self) -> Result<()> {
        let conn_guard = self.conn.lock();
        conn_guard.execute(INSERT_PAUSE, [])?;
        Ok(())
    }

    // Inserts a new pause start record with a specific timestamp.
    pub fn insert_start_with_time(&self, start_time: NaiveDateTime) -> Result<(), Box<dyn Error>> {
        let conn_guard = self.conn.lock();
        let start_str = start_time.format("%Y-%m-%d %H:%M:%S").to_string();
        conn_guard.execute(INSERT_PAUSE_WITH_TIME, [&start_str])?;
        Ok(())
    }

    // Updates the most recent open pause (end IS NULL) with an end timestamp and duration.
    pub fn insert_end(&self) -> Result<(), Box<dyn Error>> {
        let end = Local::now().naive_local();
        let conn_guard = self.conn.lock();

        let mut stmt = conn_guard.prepare(SELECT_LAST_PAUSE)?;
        let pause_row = stmt.query_row([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)));
        if let Ok((id, start_str)) = pause_row {
            let start = NaiveDateTime::parse_from_str(&start_str, "%Y-%m-%d %H:%M:%S")?;
            let duration = (end - start).num_seconds();
            conn_guard.execute(UPDATE_PAUSE, [&duration.to_string(), &id.to_string()])?;
        }
        Ok(())
    }

    // Fetches pauses for a given date, filtering by minimum duration (in minutes).
    pub fn fetch(&self, date: NaiveDate, min_duration: u64) -> Result<Vec<Pause>, Box<dyn Error>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let min_duration_secs = (min_duration * 60) as i64; // Convert minutes to seconds

        let conn_guard = self.conn.lock();
        let mut stmt = conn_guard.prepare(SELECT_DAILY_PAUSES)?;

        let pause_iter = stmt.query_map([&date_str, &min_duration_secs.to_string()], |row| {
            let start_str: String = row.get(1)?;
            let end_str: Option<String> = row.get(2)?;
            let duration: i64 = row.get(3).unwrap_or(0);
            Ok(Pause {
                id: row.get(0)?,
                start: NaiveDateTime::parse_from_str(&start_str, "%Y-%m-%d %H:%M:%S").unwrap(),
                end: end_str.map(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").unwrap()),
                duration: Some(TimeDelta::seconds(duration)),
            })
        })?;
        let mut pauses = Vec::new();
        for pause_result in pause_iter {
            pauses.push(pause_result?);
        }
        Ok(pauses)
    }
}
