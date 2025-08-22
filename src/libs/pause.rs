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

use chrono::{prelude::NaiveDateTime, Duration};

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

