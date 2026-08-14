//! Time duration formatting utilities for user-friendly display.
//!
//! Provides formatting functions and types for converting time durations into
//! human-readable string representations used throughout the application.
//!
//! ## Usage
//!
//! ```rust
//! use kasl::libs::formatter::{format_duration, FormattedEvent};
//! use chrono::Duration;
//!
//! let duration = Duration::hours(2) + Duration::minutes(30);
//! let formatted = format_duration(&duration);
//! assert_eq!(formatted, "02:30");
//! ```

use chrono::Duration;
use serde::{Deserialize, Serialize};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Represents a formatted time-based event for display purposes.
///
/// ## Examples
///
/// ```rust
/// use kasl::libs::formatter::FormattedEvent;
///
/// // Work interval representation
/// let interval = FormattedEvent {
///     id: 1,
///     start: "09:00".to_string(),
///     end: "12:00".to_string(),
///     duration: "03:00".to_string(),
/// };
///
/// // Pause representation
/// let pause = FormattedEvent {
///     id: 2,
///     start: "12:00".to_string(),
///     end: "12:30".to_string(),
///     duration: "00:30".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedEvent {
    /// The sequential identifier of the event.
    ///
    /// Used for ordering events chronologically and providing reference
    /// numbers in display tables. Typically starts from 1 and increments
    /// for each event in a sequence.
    pub id: i32,

    /// The formatted start time (e.g., "09:00", "14:30").
    ///
    /// Represents when the event began, typically formatted as "HH:MM"
    /// in 24-hour format. For work intervals, this is when work started.
    /// For pauses, this is when the break began.
    pub start: String,

    /// The formatted end time (e.g., "17:00", "15:15").
    ///
    /// Represents when the event ended, typically formatted as "HH:MM"
    /// in 24-hour format. May be "-" or empty if the event is ongoing
    /// or has no defined end time.
    pub end: String,

    /// The formatted duration (e.g., "08:00", "00:45").
    ///
    /// Represents the total length of the event, formatted as "HH:MM".
    /// This is calculated from the difference between start and end times.
    /// May be "--:--" if the duration cannot be determined.
    pub duration: String,
}

/// Formats a chrono::Duration into a standardized "HH:MM" string.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::formatter::format_duration;
/// use chrono::Duration;
///
/// // Standard durations
/// assert_eq!(format_duration(&Duration::hours(8)), "08:00");
/// assert_eq!(format_duration(&Duration::minutes(90)), "01:30");
/// assert_eq!(format_duration(&Duration::minutes(45)), "00:45");
///
/// // Edge cases
/// assert_eq!(format_duration(&Duration::zero()), "00:00");
/// assert_eq!(format_duration(&Duration::hours(-1)), "00:00");
/// assert_eq!(format_duration(&Duration::hours(24)), "24:00");
/// ```
pub fn format_duration(duration: &Duration) -> String {
    // Extract hours and minutes from the duration
    let hours = duration.num_hours();
    let mins = duration.num_minutes() % 60;

    // Ensure we don't display negative durations by clamping to zero
    // This handles edge cases where calculations might result in negative values
    format!("{:02}:{:02}", hours.max(0), mins.max(0))
}

/// Returns the current terminal width in columns, or `100` when unknown.
pub fn terminal_cols() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .filter(|&cols| cols > 0)
        .unwrap_or(100)
}

/// Truncates `s` to at most `max_width` display columns, appending `…` when cut.
///
/// Uses Unicode display width so Cyrillic and ASCII share the same budget.
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    if s.width() <= max_width {
        return s.to_string();
    }

    const ELLIPSIS: &str = "…";
    let ellipsis_width = ELLIPSIS.width();
    if max_width <= ellipsis_width {
        return ELLIPSIS.chars().take(max_width).collect();
    }

    let target = max_width - ellipsis_width;
    let mut used = 0;
    let mut end = 0;
    for (idx, ch) in s.char_indices() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > target {
            break;
        }
        used += ch_width;
        end = idx + ch.len_utf8();
    }

    format!("{}{}", &s[..end], ELLIPSIS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_leaves_short_strings_unchanged() {
        assert_eq!(truncate_to_width("hello", 10), "hello");
        assert_eq!(truncate_to_width("task name", 20), "task name");
    }

    #[test]
    fn truncate_ascii_adds_ellipsis_within_budget() {
        let truncated = truncate_to_width("abcdefghij", 7);
        assert_eq!(truncated, "abcdef…");
        assert_eq!(truncated.width(), 7);
    }

    #[test]
    fn truncate_fullwidth_respects_display_width() {
        // Fullwidth letters have display width 2 each
        let truncated = truncate_to_width("ＡＢＣＤＥＦ", 7);
        assert_eq!(truncated, "ＡＢＣ…");
        assert_eq!(truncated.width(), 7);
    }

    #[test]
    fn truncate_zero_width_returns_empty() {
        assert_eq!(truncate_to_width("hello", 0), "");
    }
}
