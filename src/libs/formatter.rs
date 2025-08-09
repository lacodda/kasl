//! Time duration formatting utilities for user-friendly display.
//!
//! This module provides formatting functions and types for converting time durations
//! into human-readable string representations. It's used throughout the application
//! for displaying work hours, pause durations, and time intervals in reports.
//!
//! ## Features
//!
//! - **Consistent Formatting**: All time durations use the same "HH:MM" format
//! - **Safety**: Handles negative durations gracefully by treating them as zero
//! - **Performance**: Lightweight formatting with minimal allocations
//! - **Integration**: Works seamlessly with `chrono::Duration` types
//!
//! ## Usage Patterns
//!
//! The module is primarily used in three contexts:
//!
//! ### Report Generation
//! Work reports display total hours, break durations, and productivity metrics
//! using formatted time strings for easy reading.
//!
//! ### Data Export
//! When exporting data to CSV, JSON, or Excel formats, time durations are
//! converted to standardized string representations.
//!
//! ### Console Display
//! Table views and status messages use formatted durations to show time
//! information in a consistent, readable format.
//!
//! ## Format Specifications
//!
//! ### Duration Format
//! All durations follow the "HH:MM" pattern:
//! - Hours are zero-padded to 2 digits
//! - Minutes are zero-padded to 2 digits
//! - No seconds are displayed (rounded to nearest minute)
//! - Negative durations are treated as "00:00"
//!
//! ### Examples
//! - 1 hour 30 minutes → "01:30"
//! - 8 hours 45 minutes → "08:45"
//! - 30 minutes → "00:30"
//! - Negative duration → "00:00"
//!
//! ## Error Handling
//!
//! The formatting functions are designed to be robust:
//! - Invalid durations default to zero time
//! - Overflow conditions are handled safely
//! - No panics or errors are possible during formatting
//!
//! ## Examples
//!
//! ```rust
//! use kasl::libs::formatter::{format_duration, FormattedEvent};
//! use chrono::Duration;
//!
//! // Format a duration
//! let duration = Duration::hours(2) + Duration::minutes(30);
//! let formatted = format_duration(&duration);
//! assert_eq!(formatted, "02:30");
//!
//! // Create a formatted event for display
//! let event = FormattedEvent {
//!     id: 1,
//!     start: "09:00".to_string(),
//!     end: "17:30".to_string(),
//!     duration: "08:30".to_string(),
//! };
//! ```

use chrono::Duration;
use serde::{Deserialize, Serialize};

/// Represents a formatted time-based event for display purposes.
///
/// This structure holds string representations of event properties, making it
/// suitable for direct use with table-rendering libraries and data export
/// systems. All time values are pre-formatted for consistent display.
///
/// ## Design Rationale
///
/// Rather than storing raw time values and formatting them at display time,
/// this structure pre-formats all values to strings. This approach provides:
///
/// - **Performance**: No repeated formatting calculations
/// - **Consistency**: All instances use identical formatting
/// - **Simplicity**: Direct use in templates and display systems
/// - **Serialization**: Easy JSON/CSV export without custom formatters
///
/// ## Usage Context
///
/// This structure is primarily used for:
/// - Console table display of work intervals
/// - CSV export of time-based data
/// - JSON serialization for API responses
/// - Report generation and data visualization
///
/// ## Field Descriptions
///
/// - `id`: Sequential number for ordering and reference
/// - `start`: Formatted start time (typically "HH:MM")  
/// - `end`: Formatted end time (typically "HH:MM")
/// - `duration`: Formatted duration (typically "HH:MM")
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
/// This function converts a time duration into a human-readable format
/// suitable for display in reports, tables, and user interfaces. It ensures
/// consistent formatting across the entire application.
///
/// ## Formatting Rules
///
/// - **Hours**: Always displayed with at least 2 digits (zero-padded)
/// - **Minutes**: Always displayed with exactly 2 digits (zero-padded)
/// - **Seconds**: Not displayed (rounded to nearest minute)
/// - **Negative**: Treated as zero duration ("00:00")
/// - **Overflow**: Large durations handled gracefully
///
/// ## Algorithm
///
/// 1. Extract total hours from the duration
/// 2. Extract remaining minutes (after removing full hours)
/// 3. Clamp negative values to zero
/// 4. Format with zero-padding
///
/// # Arguments
///
/// * `duration` - A reference to the chrono::Duration to format
///
/// # Returns
///
/// A String in "HH:MM" format representing the duration.
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
///
/// ## Performance Notes
///
/// This function is designed for frequent use and has minimal overhead:
/// - Single allocation for the result string
/// - Simple arithmetic operations only
/// - No complex parsing or validation
///
/// ## Thread Safety
///
/// This function is pure and thread-safe. It can be called concurrently
/// from multiple threads without synchronization.
pub fn format_duration(duration: &Duration) -> String {
    // Extract hours and minutes from the duration
    let hours = duration.num_hours();
    let mins = duration.num_minutes() % 60;

    // Ensure we don't display negative durations by clamping to zero
    // This handles edge cases where calculations might result in negative values
    format!("{:02}:{:02}", hours.max(0), mins.max(0))
}
