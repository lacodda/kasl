//! Manages pause data, including formatting for display.

use crate::libs::formatter::FormattedEvent;
use chrono::{prelude::NaiveDateTime, Duration, TimeDelta};

/// Represents a single pause period.
#[derive(Debug, Clone)]
pub struct Pause {
    /// The unique identifier for the pause record.
    pub id: i32,
    /// The timestamp when the pause started.
    pub start: NaiveDateTime,
    /// The timestamp when the pause ended.
    pub end: Option<NaiveDateTime>,
    /// The calculated duration of the pause.
    pub duration: Option<Duration>,
}

/// A trait for formatting a collection of `Pause` instances.
pub trait PauseGroup {
    /// Formats a vector of `Pause` into a vector of `FormattedEvent` for display.
    fn format(&mut self) -> Vec<FormattedEvent>;
}

impl PauseGroup for Vec<Pause> {
    fn format(&mut self) -> Vec<FormattedEvent> {
        self.iter()
            .enumerate()
            .map(|(index, p)| FormattedEvent {
                id: (index + 1) as i32,
                start: p.start.format("%H:%M").to_string(),
                end: p.end.map_or_else(|| "-".to_string(), |e| e.format("%H:%M").to_string()),
                duration: p
                    .duration
                    .map_or_else(|| "--:--".to_string(), |d: TimeDelta| crate::libs::formatter::format_duration(&d)),
            })
            .collect()
    }
}
