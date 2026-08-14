//! The pause model shared by the monitor, the database layer and the views.
//!
//! ```rust,no_run
//! # fn f() -> Result<(), chrono::ParseError> {
//! use kasl::libs::pause::Pause;
//! use chrono::{NaiveDateTime, Duration};
//!
//! let pause = Pause::detected(
//!     1,
//!     NaiveDateTime::parse_from_str("2025-08-11 09:15:00", "%Y-%m-%d %H:%M:%S")?,
//!     Some(NaiveDateTime::parse_from_str("2025-08-11 09:30:00", "%Y-%m-%d %H:%M:%S")?),
//!     Some(Duration::minutes(15)),
//! );
//! # Ok(())
//! # }
//! ```

use chrono::{Duration, prelude::NaiveDateTime};

/// A single break period; `end` and `duration` stay `None` while it is ongoing.
#[derive(Debug, Clone)]
pub struct Pause {
    /// Database primary key.
    pub id: i32,

    /// When the inactivity threshold was crossed (local time).
    pub start: NaiveDateTime,

    /// When activity resumed; `None` while the pause is still running.
    pub end: Option<NaiveDateTime>,

    /// `end - start`, stored by the database layer; `None` while ongoing.
    pub duration: Option<Duration>,

    /// Whether this pause was entered manually and must be preserved as-is.
    ///
    /// Protected pauses are recorded by the user through `kasl pauses add`
    /// rather than detected by the activity monitor. They are exempt from the
    /// minimum-duration threshold and are never merged with adjacent pauses,
    /// so a deliberately short entry (a ten-minute walk the monitor missed)
    /// survives filtering intact.
    pub protected: bool,
}

impl Pause {
    /// Builds a monitor-detected pause (not protected).
    ///
    /// Used when reconstructing pauses from sources that carry no protection
    /// flag, such as in-memory analysis of activity data.
    pub fn detected(id: i32, start: NaiveDateTime, end: Option<NaiveDateTime>, duration: Option<Duration>) -> Self {
        Self {
            id,
            start,
            end,
            duration,
            protected: false,
        }
    }
}
