//! Days owed to kasl-server: what could not be sent, kept until it can.
//!
//! A row here is a date, never a payload. The day is rebuilt from the local
//! tables at the moment it is finally sent, so a correction made while the
//! network was down is the version that arrives - which is also the rule the
//! server plays by, where the last upload wins (ADR 0004 in kasl-server).
//!
//! Storing the assembled JSON instead would freeze the day at the moment it
//! first failed, and an employee who fixed a task on Tuesday would watch the
//! broken Monday copy land on Friday.
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use kasl::db::server_outbox::ServerOutbox;
//! use chrono::Local;
//!
//! let mut outbox = ServerOutbox::new()?;
//! outbox.enqueue(Local::now().date_naive(), "the server could not be reached")?;
//!
//! for owed in outbox.pending()? {
//!     println!("{} is still owed", owed.date);
//! }
//! # Ok(())
//! # }
//! ```

use crate::db::db::Db;
use anyhow::Result;
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::Connection;

const SCHEMA_SERVER_OUTBOX: &str = "CREATE TABLE IF NOT EXISTS server_outbox (
    id INTEGER PRIMARY KEY,
    date DATE NOT NULL UNIQUE,
    queued_at TIMESTAMP NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_attempt_at TIMESTAMP,
    last_error TEXT
);";

/// Records a day as owed, or notes another failed attempt at one already
/// recorded.
///
/// `ON CONFLICT` is what keeps a fortnight offline from becoming a fortnight
/// of duplicate rows for the same date: the debt is the date, and failing to
/// pay it again does not create a second one. `queued_at` is deliberately not
/// touched on conflict - it answers "how long has this been stuck", which a
/// refresh on every retry would erase.
const ENQUEUE: &str = "INSERT INTO server_outbox (date, queued_at, attempts, last_attempt_at, last_error)
     VALUES (?1, datetime(CURRENT_TIMESTAMP, 'localtime'), 1, datetime(CURRENT_TIMESTAMP, 'localtime'), ?2)
     ON CONFLICT(date) DO UPDATE SET
        attempts = attempts + 1,
        last_attempt_at = datetime(CURRENT_TIMESTAMP, 'localtime'),
        last_error = ?2";

const SELECT_PENDING: &str = "SELECT id, date, queued_at, attempts, last_attempt_at, last_error FROM server_outbox ORDER BY date ASC";

const DELETE_BY_DATE: &str = "DELETE FROM server_outbox WHERE date = ?1";

const COUNT_PENDING: &str = "SELECT COUNT(*) FROM server_outbox";

/// One day still owed to the server.
#[derive(Debug, Clone)]
pub struct OwedDay {
    /// Database primary key.
    pub id: i32,

    /// The date whose day has not been delivered.
    pub date: NaiveDate,

    /// When the day was first found undeliverable. Kept across retries, so
    /// the age of the debt stays readable.
    pub queued_at: NaiveDateTime,

    /// How many times delivery has been tried.
    pub attempts: i32,

    /// When it was last tried.
    pub last_attempt_at: Option<NaiveDateTime>,

    /// What went wrong last time, as the user would read it.
    pub last_error: Option<String>,
}

/// Access to the outbox table.
pub struct ServerOutbox {
    pub conn: Connection,
}

impl ServerOutbox {
    /// Opens the database and ensures the outbox table exists.
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        db.conn.execute(SCHEMA_SERVER_OUTBOX, [])?;
        Ok(ServerOutbox { conn: db.conn })
    }

    /// Records `date` as owed, or notes another failed attempt at it.
    ///
    /// Safe to call for a date already queued: the row is updated rather than
    /// duplicated.
    pub fn enqueue(&mut self, date: NaiveDate, error: &str) -> Result<()> {
        self.conn.execute(ENQUEUE, rusqlite::params![date, error])?;
        Ok(())
    }

    /// Every day still owed, oldest first.
    ///
    /// Oldest first because a backlog is delivered in the order it happened:
    /// a dashboard filling in from the far end reads as a machine catching
    /// up, one filling in at random reads as a machine malfunctioning.
    pub fn pending(&self) -> Result<Vec<OwedDay>> {
        let mut statement = self.conn.prepare(SELECT_PENDING)?;
        let rows = statement.query_map([], |row| {
            Ok(OwedDay {
                id: row.get(0)?,
                date: row.get(1)?,
                queued_at: row.get(2)?,
                attempts: row.get(3)?,
                last_attempt_at: row.get(4)?,
                last_error: row.get(5)?,
            })
        })?;

        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    /// How many days are owed.
    pub fn count(&self) -> Result<i64> {
        Ok(self.conn.query_row(COUNT_PENDING, [], |row| row.get(0))?)
    }

    /// Forgets `date`, whether it was delivered or given up on.
    ///
    /// One method for both outcomes on purpose: the queue's business is
    /// whether a day is still owed, and a day the server will never accept is
    /// no more owed than one it has taken. Which of the two happened is said
    /// out loud by the caller, where the user can see it.
    pub fn remove(&mut self, date: NaiveDate) -> Result<bool> {
        Ok(self.conn.execute(DELETE_BY_DATE, rusqlite::params![date])? > 0)
    }
}
