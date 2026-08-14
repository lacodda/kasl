//! Workday records: one row per calendar date, start and optional end.
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use kasl::db::workdays::Workdays;
//! use chrono::Local;
//!
//! let mut workdays = Workdays::new()?;
//! let today = Local::now().date_naive();
//!
//! workdays.insert_start(today)?;
//! workdays.insert_end(today)?;
//! # Ok(())
//! # }
//! ```

use crate::{db::db::Db, libs::messages::Message, msg_error_anyhow};
use anyhow::Result;
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OptionalExtension};

const SCHEMA_WORKDAYS: &str = "CREATE TABLE IF NOT EXISTS workdays (
    id INTEGER PRIMARY KEY,
    date DATE NOT NULL UNIQUE,
    start TIMESTAMP NOT NULL,
    end TIMESTAMP
);";

const INSERT_START: &str = "INSERT INTO workdays (date, start) VALUES (?1, datetime(CURRENT_TIMESTAMP, 'localtime'))";
const UPDATE_END: &str = "UPDATE workdays SET end = datetime(CURRENT_TIMESTAMP, 'localtime') WHERE date = ?1";
const SELECT_BY_DATE: &str = "SELECT id, date, start, end FROM workdays WHERE date = ?1";
const SELECT_BY_MONTH: &str = "SELECT id, date, start, end FROM workdays WHERE strftime('%Y-%m', date) = strftime('%Y-%m', ?1)";
const UPDATE_START: &str = "UPDATE workdays SET start = ?1 WHERE date = ?2";
const UPDATE_END_TIME: &str = "UPDATE workdays SET end = ?1 WHERE date = ?2";
const UNSET_END_TIME: &str = "UPDATE workdays SET end = NULL WHERE date = ?1";

/// One day's work session. Timestamps are local time; `end: None` means the
/// session is still open.
#[derive(Debug, Clone)]
pub struct Workday {
    /// Database primary key.
    pub id: i32,

    /// Calendar date; unique per row.
    pub date: NaiveDate,

    /// When the session began.
    pub start: NaiveDateTime,

    /// When the session ended; `None` while it is open.
    pub end: Option<NaiveDateTime>,
}

/// Workday table access.
pub struct Workdays {
    pub conn: Connection,
}

impl Workdays {
    /// Opens the database and ensures the workdays table exists.
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::db::workdays::Workdays;
    ///
    /// let mut workdays = Workdays::new()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        db.conn.execute(SCHEMA_WORKDAYS, [])?;
        Ok(Workdays { conn: db.conn })
    }

    /// Starts a workday at the current time; a no-op if the date already has
    /// one, so repeated calls from the monitor are safe.
    ///
    /// ```rust,no_run
    /// # use kasl::db::workdays::Workdays;
    /// use chrono::Local;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    /// workdays.insert_start(today)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn insert_start(&mut self, date: NaiveDate) -> Result<()> {
        let date_str = date.format("%Y-%m-%d").to_string();
        if self.fetch(date)?.is_none() {
            self.conn.execute(INSERT_START, [&date_str])?;
        }
        Ok(())
    }

    /// Stamps the current time as the day's end; calling again re-stamps.
    ///
    /// Known gap: when no workday exists for the date, the UPDATE matches
    /// nothing and this still returns `Ok` - `kasl end` then reports success
    /// without having written anything (tracked for the doctor stage).
    ///
    /// ```rust,no_run
    /// # use kasl::db::workdays::Workdays;
    /// use chrono::Local;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    ///
    /// workdays.insert_start(today)?;
    /// workdays.insert_end(today)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn insert_end(&mut self, date: NaiveDate) -> Result<()> {
        let date_str = date.format("%Y-%m-%d").to_string();
        self.conn.execute(UPDATE_END, [&date_str])?;
        Ok(())
    }

    /// Fetches the workday for a date, or `None` if the date has none.
    ///
    /// ```rust,no_run
    /// # use kasl::db::workdays::Workdays;
    /// use chrono::Local;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    ///
    /// if let Some(workday) = workdays.fetch(today)? {
    ///     println!("Work started at: {}", workday.start);
    ///     if let Some(end_time) = workday.end {
    ///         println!("Work ended at: {}", end_time);
    ///     } else {
    ///         println!("Work session is still active");
    ///     }
    /// } else {
    ///     println!("No work session recorded for today");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn fetch(&mut self, date: NaiveDate) -> Result<Option<Workday>> {
        let date_str = date.format("%Y-%m-%d").to_string();

        let workday = self
            .conn
            .query_row(SELECT_BY_DATE, [&date_str], |row| {
                Ok(Workday {
                    id: row.get(0)?,
                    date: NaiveDate::parse_from_str(&row.get::<_, String>(1)?, "%Y-%m-%d").unwrap(),
                    start: NaiveDateTime::parse_from_str(&row.get::<_, String>(2)?, "%Y-%m-%d %H:%M:%S").unwrap(),
                    end: row
                        .get::<_, Option<String>>(3)?
                        .map(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").unwrap()),
                })
            })
            .optional()?;

        Ok(workday)
    }

    /// Fetches every workday in the calendar month containing `date`.
    ///
    /// ```rust,no_run
    /// # use kasl::db::workdays::Workdays;
    /// use chrono::Local;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let mut workdays = Workdays::new()?;
    /// let current_month = Local::now().date_naive();
    ///
    /// let monthly_workdays = workdays.fetch_month(current_month)?;
    /// println!("Found {} workdays this month", monthly_workdays.len());
    ///
    /// for workday in monthly_workdays {
    ///     if let Some(end_time) = workday.end {
    ///         let duration = end_time - workday.start;
    ///         println!("Date: {}, Duration: {:?}", workday.date, duration);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn fetch_month(&mut self, date: NaiveDate) -> Result<Vec<Workday>> {
        let date_str = date.format("%Y-%m-%d").to_string();

        let mut stmt = self.conn.prepare(SELECT_BY_MONTH)?;
        let workday_iter = stmt.query_map([&date_str], |row| {
            Ok(Workday {
                id: row.get(0)?,
                date: NaiveDate::parse_from_str(&row.get::<_, String>(1)?, "%Y-%m-%d").unwrap(),
                start: NaiveDateTime::parse_from_str(&row.get::<_, String>(2)?, "%Y-%m-%d %H:%M:%S").unwrap(),
                end: row
                    .get::<_, Option<String>>(3)?
                    .map(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").unwrap()),
            })
        })?;

        let mut workdays = Vec::new();
        for workday in workday_iter {
            workdays.push(workday?);
        }

        Ok(workdays)
    }

    /// Sets the day's start to a specific timestamp; errors if the date has
    /// no workday.
    ///
    /// ```rust,no_run
    /// # use kasl::db::workdays::Workdays;
    /// use chrono::{Local, NaiveDateTime};
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    ///
    /// let corrected_start = NaiveDateTime::parse_from_str(
    ///     &format!("{} 09:00:00", today.format("%Y-%m-%d")),
    ///     "%Y-%m-%d %H:%M:%S"
    /// )?;
    ///
    /// workdays.update_start(today, corrected_start)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn update_start(&mut self, date: NaiveDate, new_start: NaiveDateTime) -> Result<()> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let start_str = new_start.format("%Y-%m-%d %H:%M:%S").to_string();

        let affected = self.conn.execute(UPDATE_START, [&start_str, &date_str])?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::WorkdayUpdateFailed));
        }

        Ok(())
    }

    /// Sets the day's end to a specific timestamp, or reopens the day with
    /// `None`; errors if the date has no workday.
    ///
    /// ```rust,no_run
    /// # use kasl::db::workdays::Workdays;
    /// use chrono::{Local, NaiveDateTime};
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    ///
    /// let end_time = NaiveDateTime::parse_from_str(
    ///     &format!("{} 17:30:00", today.format("%Y-%m-%d")),
    ///     "%Y-%m-%d %H:%M:%S"
    /// )?;
    /// workdays.update_end(today, Some(end_time))?;
    ///
    /// // Reopen the day
    /// workdays.update_end(today, None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn update_end(&mut self, date: NaiveDate, new_end: Option<NaiveDateTime>) -> Result<()> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let end_str = new_end.map(|e| e.format("%Y-%m-%d %H:%M:%S").to_string());

        let affected = match end_str {
            Some(end) => self.conn.execute(UPDATE_END_TIME, [&end, &date_str])?,
            None => self.conn.execute(UNSET_END_TIME, [&date_str])?,
        };

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::WorkdayUpdateFailed));
        }

        Ok(())
    }
}
