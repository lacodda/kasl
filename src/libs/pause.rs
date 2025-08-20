//! Pause data management and formatting utilities.
//!
//! Provides the core data structures and formatting functionality for handling
//! break periods detected by the activity monitor.
//!
//! ## Features
//!
//! - **Data Modeling**: Represents pause periods with precise timing information
//! - **Display Formatting**: Converts raw pause data into human-readable formats
//! - **Collection Processing**: Batch operations on pause collections
//! - **Report Integration**: Provides data structures suitable for reporting systems
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::pause::Pause;
//! use chrono::{NaiveDateTime, Duration};
//!
//! let pause = Pause {
//!     id: 1,
//!     start: NaiveDateTime::parse_from_str("2025-08-11 09:15:00", "%Y-%m-%d %H:%M:%S")?,
//!     end: Some(NaiveDateTime::parse_from_str("2025-08-11 09:30:00", "%Y-%m-%d %H:%M:%S")?),
//!     duration: Some(Duration::minutes(15)),
//! };
//! ```

use crate::libs::formatter::FormattedEvent;
use chrono::{prelude::NaiveDateTime, Duration, TimeDelta};

/// Represents a single pause period with complete timing information.
///
/// This structure models a break period detected by the activity monitor,
/// containing all necessary information for analysis, reporting, and display.
/// It handles both active (ongoing) and completed pause periods gracefully.
///
/// ## Field Semantics
///
/// - **`id`**: Unique database identifier for pause tracking and updates
/// - **`start`**: Precise timestamp when inactivity threshold was reached
/// - **`end`**: Completion timestamp (None for ongoing pauses)
/// - **`duration`**: Calculated break duration (None for ongoing pauses)
///
/// ## State Handling
///
/// The structure supports two primary states:
///
/// ### Completed Pause
/// - All fields populated with meaningful values
/// - `end` contains actual completion timestamp
/// - `duration` contains calculated break duration
/// - Ready for reporting and analysis
///
/// ### Ongoing Pause (Active)
/// - `start` field contains pause initiation time
/// - `end` field is None (pause still in progress)
/// - `duration` field is None (cannot calculate until completion)
/// - Can be displayed with special formatting for active state
///
/// ## Duration Calculation
///
/// Durations are calculated and stored at the database level for consistency:
/// - **Precision**: Calculated to the second for accurate reporting
/// - **Storage**: Stored as seconds in database, converted to Duration for display
/// - **Consistency**: All duration calculations use same algorithm
/// - **Timezone**: Uses local time for user-friendly display
///
/// ## Display Considerations
///
/// The structure is designed for easy formatting:
/// - Timestamps use standard format suitable for parsing
/// - Duration is compatible with `chrono::Duration` formatting utilities
/// - None values are handled gracefully in display formatting
/// - Supports both detailed and summary display modes
#[derive(Debug, Clone)]
pub struct Pause {
    /// The unique identifier for the pause record in the database.
    ///
    /// This ID is auto-generated when the pause record is created and
    /// serves as the primary key for all database operations. It's used
    /// for updating pause records when they end and for referencing
    /// specific pauses in reports and analysis.
    pub id: i32,

    /// The timestamp when the pause period began.
    ///
    /// This represents the precise moment when the activity monitor
    /// detected that the user had been inactive for the configured
    /// pause threshold duration. The timestamp uses local time zone
    /// for user-friendly display and reporting.
    ///
    /// ## Precision
    /// - Stored with second-level precision in the database
    /// - Captured when inactivity threshold is first exceeded
    /// - Uses system local time for consistency with user expectations
    pub start: NaiveDateTime,

    /// The timestamp when the pause period ended.
    ///
    /// This field is populated when user activity resumes after a break.
    /// It remains None for ongoing pauses that haven't completed yet.
    ///
    /// ## State Semantics
    /// - **Some(timestamp)**: Pause has completed, duration can be calculated
    /// - **None**: Pause is still active, user hasn't returned yet
    ///
    /// ## Update Process
    /// The field is updated when the monitor detects resumed activity:
    /// 1. Activity monitor detects keyboard/mouse input
    /// 2. Database record is updated with current timestamp
    /// 3. Duration is calculated and stored simultaneously
    pub end: Option<NaiveDateTime>,

    /// The calculated duration of the pause period.
    ///
    /// This represents the total time the user was away from the computer
    /// during this break period. The duration is calculated automatically
    /// when the pause ends and stored for efficient reporting.
    ///
    /// ## Calculation Details
    /// - **Formula**: `end_time - start_time`
    /// - **Precision**: Second-level accuracy for detailed analysis
    /// - **Storage**: Converted from seconds to Duration for easy manipulation
    /// - **Filtering**: Only pauses meeting minimum duration are typically displayed
    ///
    /// ## None Handling
    /// The field is None for ongoing pauses where the end time hasn't been
    /// determined yet. Display formatting handles this gracefully by showing
    /// placeholder values or real-time duration calculation.
    pub duration: Option<Duration>,
}

/// Trait for formatting collections of pause data into display-ready formats.
///
/// This trait provides a standardized interface for converting pause data
/// into formats suitable for terminal display, report generation, and data
/// export. It abstracts the formatting logic to allow consistent presentation
/// across different parts of the application.
///
/// ## Design Philosophy
///
/// The trait follows the principle of data transformation pipelines:
/// 1. **Raw Data**: Database records with precise timestamps
/// 2. **Structured Data**: Parsed into Pause structures
/// 3. **Formatted Data**: Converted to display-ready FormattedEvent structures
/// 4. **Presentation**: Rendered in tables, reports, or export formats
///
/// ## Implementation Strategy
///
/// Implementations should handle:
/// - **Null Handling**: Graceful formatting of ongoing pauses
/// - **Index Generation**: Sequential numbering for display purposes
/// - **Time Formatting**: Consistent "HH:MM" format for readability
/// - **Duration Display**: Human-readable duration representation
///
/// ## Error Handling
///
/// Formatting operations should be resilient:
/// - Invalid timestamps are handled with placeholder values
/// - Missing data is represented with standard placeholders
/// - Formatting errors don't prevent processing of other records
pub trait PauseGroup {
    /// Formats a collection of pauses into display-ready events.
    ///
    /// This method transforms raw pause data into a standardized format
    /// suitable for table display, report generation, and data export.
    /// It handles the conversion of timestamps to readable formats and
    /// provides sequential indexing for user interface purposes.
    ///
    /// ## Transformation Process
    ///
    /// For each pause in the collection:
    /// 1. **Index Assignment**: Generates sequential display numbers (1, 2, 3...)
    /// 2. **Time Formatting**: Converts timestamps to "HH:MM" format
    /// 3. **Duration Formatting**: Converts Duration to "HH:MM" representation
    /// 4. **Null Handling**: Provides placeholders for incomplete data
    ///
    /// ## Output Format
    ///
    /// The returned FormattedEvent structures contain:
    /// - **id**: Sequential display number (not database ID)
    /// - **start**: Start time in "HH:MM" format
    /// - **end**: End time in "HH:MM" format or "-" for ongoing
    /// - **duration**: Duration in "HH:MM" format or "--:--" for ongoing
    ///
    /// ## Usage Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::pause::PauseGroup;
    ///
    /// let mut pauses = vec![pause1, pause2, pause3];
    /// let formatted = pauses.format();
    ///
    /// // Now ready for display in tables or reports
    /// for event in formatted {
    ///     println!("{}: {} - {} ({})", event.id, event.start, event.end, event.duration);
    /// }
    /// ```
    ///
    /// # Returns
    ///
    /// Returns a vector of FormattedEvent structures ready for display
    /// or further processing by presentation layers.
    fn format(&mut self) -> Vec<FormattedEvent>;
}

impl PauseGroup for Vec<Pause> {
    /// Formats a vector of pauses into display-ready FormattedEvent structures.
    ///
    /// This implementation provides the standard formatting behavior for pause
    /// collections, converting raw database records into user-friendly display
    /// formats with consistent styling and placeholder handling.
    ///
    /// ## Implementation Details
    ///
    /// ### Sequential Indexing
    /// - Uses 1-based indexing for user-friendly display (not 0-based)
    /// - Index represents display order, not database ID
    /// - Useful for "pause #1", "pause #2" type references in UI
    ///
    /// ### Time Formatting
    /// - Start times always formatted as "HH:MM" (24-hour format)
    /// - End times use "HH:MM" for completed pauses, "-" for ongoing
    /// - Consistent format makes data easy to scan and compare
    ///
    /// ### Duration Handling
    /// - Completed pauses show duration in "HH:MM" format
    /// - Ongoing pauses show "--:--" placeholder
    /// - Uses the shared formatter for consistency across application
    ///
    /// ### Error Resilience
    /// - Invalid timestamps result in placeholder values
    /// - Continues processing even if individual records have issues
    /// - Ensures consistent output format regardless of data quality
    ///
    /// ## Memory Efficiency
    ///
    /// The implementation:
    /// - Processes records in-place using iterator chains
    /// - Allocates result vector with known capacity
    /// - Minimizes string allocations through efficient formatting
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::pause::{Pause, PauseGroup};
    /// use chrono::{NaiveDateTime, Duration};
    ///
    /// let mut pauses = vec![
    ///     Pause {
    ///         id: 1,
    ///         start: NaiveDateTime::parse_from_str("2025-08-11 09:15:00", "%Y-%m-%d %H:%M:%S")?,
    ///         end: Some(NaiveDateTime::parse_from_str("2025-08-11 09:30:00", "%Y-%m-%d %H:%M:%S")?),
    ///         duration: Some(Duration::minutes(15)),
    ///     },
    ///     // ... more pauses
    /// ];
    ///
    /// let formatted = pauses.format();
    /// assert_eq!(formatted[0].start, "09:15");
    /// assert_eq!(formatted[0].end, "09:30");
    /// assert_eq!(formatted[0].duration, "00:15");
    /// ```
    ///
    /// # Returns
    ///
    /// A vector of FormattedEvent structures with:
    /// - Sequential IDs starting from 1
    /// - Formatted time strings in "HH:MM" format
    /// - Appropriate placeholders for missing data
    fn format(&mut self) -> Vec<FormattedEvent> {
        self.iter()
            .enumerate()
            .map(|(index, pause)| {
                // Generate user-friendly sequential ID (1-based indexing)
                let display_id = (index + 1) as i32;

                // Format start time consistently as HH:MM
                let start_formatted = pause.start.format("%H:%M").to_string();

                // Format end time with placeholder for ongoing pauses
                let end_formatted = pause
                    .end
                    .map(|end_time| end_time.format("%H:%M").to_string())
                    .unwrap_or_else(|| "-".to_string());

                // Format duration with placeholder for ongoing pauses
                let duration_formatted = pause
                    .duration
                    .map(|duration: TimeDelta| {
                        // Use the shared duration formatter for consistency
                        crate::libs::formatter::format_duration(&duration)
                    })
                    .unwrap_or_else(|| "--:--".to_string());

                // Create formatted event structure
                FormattedEvent {
                    id: display_id,
                    start: start_formatted,
                    end: end_formatted,
                    duration: duration_formatted,
                }
            })
            .collect()
    }
}
