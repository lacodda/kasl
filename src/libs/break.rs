//! Manages break data, including formatting for display.

use crate::libs::formatter::{format_duration, FormattedEvent};
use chrono::{prelude::NaiveDateTime, Duration, TimeDelta};

/// Represents a single break period.
#[derive(Debug, Clone)]
pub struct Break {
    /// The unique identifier for the break record.
    pub id: i32,
    /// The timestamp when the break started.
    pub start: NaiveDateTime, // TIMESTAMP as YYYY-MM-DD HH:MM:SS
    /// The timestamp when the break ended.
    pub end: Option<NaiveDateTime>, // TIMESTAMP as YYYY-MM-DD HH:MM:SS
    /// The calculated duration of the break.
    pub duration: Option<Duration>, // Duration in seconds
}

/// A trait for formatting a collection of `Break` instances.
pub trait BreakGroup {
    /// Formats a vector of `Break` into a vector of `FormattedEvent` for display.
    fn format(&mut self) -> Vec<FormattedEvent>;
}

impl BreakGroup for Vec<Break> {
    fn format(&mut self) -> Vec<FormattedEvent> {
        self.iter()
            .enumerate()
            .map(|(index, b)| FormattedEvent {
                id: (index + 1) as i32,
                start: b.start.format("%H:%M").to_string(),
                end: b.end.map_or_else(|| "-".to_string(), |e| e.format("%H:%M").to_string()),
                duration: b.duration.map_or_else(|| "--:--".to_string(), |d: TimeDelta| format_duration(&d)),
            })
            .collect()
    }
}
