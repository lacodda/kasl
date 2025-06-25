//! Provides shared formatting logic for display purposes.
//!
//! This module contains helper functions and structs used to format data,
//! such as time durations and event-like structures, before they are
//! rendered in the console view.

use chrono::Duration;
use serde::{Deserialize, Serialize};

/// Represents a formatted time-based event for display.
///
/// This struct holds string representations of event properties, making it
/// suitable for direct use with table-rendering libraries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedEvent {
    /// The sequential identifier of the event.
    pub id: i32,
    /// The formatted start time (e.g., "HH:MM").
    pub start: String,
    /// The formatted end time (e.g., "HH:MM").
    pub end: String,
    /// The formatted duration (e.g., "HH:MM").
    pub duration: String,
}

/// Formats a `chrono::Duration` into a "HH:MM" string.
///
/// If the duration is negative, it will be treated as zero.
///
/// # Arguments
///
/// * `duration` - A reference to the `Duration` to format.
///
/// # Returns
///
/// A `String` in "HH:MM" format.
pub fn format_duration(duration: &Duration) -> String {
    let hours = duration.num_hours();
    let mins = duration.num_minutes() % 60;
    // Ensure we don't display negative durations
    format!("{:02}:{:02}", hours.max(0), mins.max(0))
}
