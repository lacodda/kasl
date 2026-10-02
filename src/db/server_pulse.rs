//! The last pulse this machine sent to kasl-server, and how it went.
//!
//! The watcher sends the pulse and `kasl server status` reports on it, and
//! the two are different processes: this row is how one tells the other. It
//! holds the latest attempt and nothing behind it - the server keeps only the
//! latest claim, and a local history of when someone was at their desk would
//! be the very record the pulse was designed not to leave (ADR 0014 in
//! kasl-server).
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use chrono::{Duration, Utc};
//! use kasl::api::kasl_server::AgentState;
//! use kasl::db::server_pulse::ServerPulse;
//!
//! let mut pulse = ServerPulse::new()?;
//! let now = Utc::now();
//! pulse.record_accepted(now, now + Duration::seconds(60), AgentState::Working, 180, 0)?;
//!
//! if let Some(last) = pulse.last()? {
//!     println!("last attempt at {}", last.attempted_at);
//! }
//! # Ok(())
//! # }
//! ```

use crate::api::kasl_server::AgentState;
use crate::db::db::Db;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};

/// An accepted pulse replaces everything, the error included.
const RECORD_ACCEPTED: &str = "INSERT INTO server_pulse
        (id, attempted_at, next_at, sent_at, state, stale_after_seconds, clock_skew_seconds, error)
     VALUES (1, ?1, ?2, ?1, ?3, ?4, ?5, NULL)
     ON CONFLICT(id) DO UPDATE SET
        attempted_at = excluded.attempted_at,
        next_at = excluded.next_at,
        sent_at = excluded.sent_at,
        state = excluded.state,
        stale_after_seconds = excluded.stale_after_seconds,
        clock_skew_seconds = excluded.clock_skew_seconds,
        error = NULL";

/// A failed attempt leaves the last accepted pulse in place: "the last one
/// that arrived" is what someone reading a failure needs next.
const RECORD_FAILED: &str = "INSERT INTO server_pulse (id, attempted_at, next_at, error)
     VALUES (1, ?1, ?2, ?3)
     ON CONFLICT(id) DO UPDATE SET
        attempted_at = excluded.attempted_at,
        next_at = excluded.next_at,
        error = excluded.error";

const SELECT_LAST: &str = "SELECT attempted_at, next_at, sent_at, state, stale_after_seconds, clock_skew_seconds, error FROM server_pulse WHERE id = 1";

const DELETE_ALL: &str = "DELETE FROM server_pulse";

/// The latest attempt, as the watcher recorded it.
#[derive(Debug, Clone, PartialEq)]
pub struct PulseRecord {
    /// When the watcher last tried, whatever came of it.
    pub attempted_at: DateTime<Utc>,

    /// When it means to try next. A watcher that is gone stops moving this,
    /// which is how a reader tells "quiet because nothing changed" from
    /// "quiet because nobody is sending".
    pub next_at: DateTime<Utc>,

    /// When a pulse last arrived; `None` until one has.
    pub sent_at: Option<DateTime<Utc>>,

    /// The state that pulse claimed.
    pub state: Option<AgentState>,

    /// How long the server believes a pulse, from its last answer.
    pub stale_after_seconds: Option<i64>,

    /// This machine's clock against the server's, from its last answer.
    pub clock_skew_seconds: Option<i64>,

    /// Why the last attempt did not arrive; `None` when it did.
    pub error: Option<String>,
}

/// Access to the pulse row.
pub struct ServerPulse {
    pub conn: Connection,
}

impl ServerPulse {
    /// Opens the database; the table comes from the migrations.
    pub fn new() -> Result<Self> {
        Ok(ServerPulse { conn: Db::new()?.conn })
    }

    /// Records a pulse the server accepted.
    pub fn record_accepted(
        &mut self,
        at: DateTime<Utc>,
        next_at: DateTime<Utc>,
        state: AgentState,
        stale_after_seconds: i64,
        clock_skew_seconds: i64,
    ) -> Result<()> {
        self.conn.execute(
            RECORD_ACCEPTED,
            rusqlite::params![at, next_at, state.as_str(), stale_after_seconds, clock_skew_seconds],
        )?;
        Ok(())
    }

    /// Records an attempt that did not arrive, and why.
    pub fn record_failed(&mut self, at: DateTime<Utc>, next_at: DateTime<Utc>, error: &str) -> Result<()> {
        self.conn.execute(RECORD_FAILED, rusqlite::params![at, next_at, error])?;
        Ok(())
    }

    /// The latest attempt, or `None` when there has been none since the pulse
    /// was last turned on.
    pub fn last(&self) -> Result<Option<PulseRecord>> {
        let record = self
            .conn
            .query_row(SELECT_LAST, [], |row| {
                let state: Option<String> = row.get(3)?;
                Ok(PulseRecord {
                    attempted_at: row.get(0)?,
                    next_at: row.get(1)?,
                    sent_at: row.get(2)?,
                    state: state.as_deref().and_then(AgentState::parse),
                    stale_after_seconds: row.get(4)?,
                    clock_skew_seconds: row.get(5)?,
                    error: row.get(6)?,
                })
            })
            .optional()?;
        Ok(record)
    }

    /// Forgets the record, so what is reported next belongs to a pulse sent
    /// after this moment rather than to one from before it was turned off.
    pub fn clear(&mut self) -> Result<()> {
        self.conn.execute(DELETE_ALL, [])?;
        Ok(())
    }
}
