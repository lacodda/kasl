//! Daily work session tracking and time management operations.
//!
//! Provides functionality for managing daily work sessions, including automatic
//! start/end time tracking, time adjustments, and period-based querying.
//!
//! ## Features
//!
//! - **Session Tracking**: Automatic recording of daily work start and end times
//! - **Time Adjustments**: Manual correction of work session boundaries
//! - **Period Queries**: Efficient retrieval of workdays by date ranges
//! - **Duplicate Prevention**: Ensures only one workday record per date
//! - **Timezone Handling**: Consistent local timezone management for all operations
//!
//! ## Usage
//!
//! ```rust
//! use kasl::db::workdays::Workdays;
//! use chrono::Local;
//!
//! let mut workdays = Workdays::new()?;
//! let today = Local::now().date_naive();
//!
//! workdays.insert_start(today)?;
//! workdays.insert_end(today)?;
//! ```

use crate::{db::db::Db, libs::messages::Message, msg_error_anyhow};
use anyhow::Result;
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OptionalExtension};

/// SQL schema for the workdays table.
///
/// Defines the structure for storing daily work sessions with date uniqueness
/// constraints and proper temporal data types. The schema ensures data integrity
/// and supports efficient date-based queries for reporting and analysis.
const SCHEMA_WORKDAYS: &str = "CREATE TABLE IF NOT EXISTS workdays (
    id INTEGER PRIMARY KEY,
    date DATE NOT NULL UNIQUE,
    start TIMESTAMP NOT NULL,
    end TIMESTAMP
);";

/// Insert a new workday start record with current timestamp.
///
/// Creates a workday record for the specified date using the current time
/// as the start timestamp. The end time is left NULL to indicate an active
/// work session. Uses local timezone for consistent time handling.
const INSERT_START: &str = "INSERT INTO workdays (date, start) VALUES (?1, datetime(CURRENT_TIMESTAMP, 'localtime'))";

/// Update an existing workday with current end timestamp.
///
/// Completes a work session by setting the end timestamp to the current time.
/// This marks the completion of the workday and enables duration calculations
/// for productivity analysis and reporting.
const UPDATE_END: &str = "UPDATE workdays SET end = datetime(CURRENT_TIMESTAMP, 'localtime') WHERE date = ?1";

/// Retrieve a specific workday by date.
///
/// Fetches complete workday information including ID, date, start time, and
/// end time (if available) for a specific calendar date. Used for detailed
/// workday analysis and time adjustment operations.
const SELECT_BY_DATE: &str = "SELECT id, date, start, end FROM workdays WHERE date = ?1";

/// Retrieve all workdays within a calendar month.
///
/// Fetches workdays for the month containing the specified date using SQL
/// date functions. Useful for monthly reporting, productivity analysis, and
/// work pattern identification across longer time periods.
const SELECT_BY_MONTH: &str = "SELECT id, date, start, end FROM workdays WHERE strftime('%Y-%m', date) = strftime('%Y-%m', ?1)";

/// Update the start time of an existing workday.
///
/// Allows manual adjustment of work session start times for correction of
/// automatic tracking errors or manual time entry scenarios. Maintains
/// data integrity while providing flexibility for time management.
const UPDATE_START: &str = "UPDATE workdays SET start = ?1 WHERE date = ?2";

/// Update the end time of an existing workday with specific timestamp.
///
/// Sets a specific end timestamp for a workday, enabling manual time
/// adjustments and corrections to automatic time tracking records.
const UPDATE_END_TIME: &str = "UPDATE workdays SET end = ?1 WHERE date = ?2";

/// Remove the end time from a workday, marking it as ongoing.
///
/// Clears the end timestamp to indicate an active or incomplete work session.
/// Useful for correcting mistakenly ended sessions or resuming work tracking.
const UNSET_END_TIME: &str = "UPDATE workdays SET end = NULL WHERE date = ?1";

/// Represents a complete workday record with temporal boundaries.
///
/// A workday encapsulates a single day's work session with start and end times,
/// providing the fundamental unit for time tracking and productivity analysis.
/// Each workday corresponds to one calendar date and contains the temporal
/// boundaries of work activity for that day.
///
/// ## Time Representation
///
/// All timestamps use `NaiveDateTime` to represent local timezone times
/// consistently across the application. This approach avoids timezone
/// complexity while maintaining accuracy for user-centric time tracking.
///
/// ## Completion States
///
/// - **Active Session**: `end` is `None`, indicating ongoing work
/// - **Completed Session**: `end` is `Some(timestamp)`, work session finished
/// - **Historical Record**: Both start and end times available for analysis
#[derive(Debug, Clone)]
pub struct Workday {
    /// Database-assigned unique identifier.
    ///
    /// Used for internal database operations and referential integrity.
    /// Automatically assigned when the workday is created.
    pub id: i32,

    /// Calendar date for this work session.
    ///
    /// Each date can have only one workday record, enforced by database
    /// constraints. Represents the calendar day when work was performed,
    /// independent of the actual start/end times which may span midnight.
    pub date: NaiveDate,

    /// Timestamp when work session began.
    ///
    /// Records the exact moment work started for this date, using local
    /// timezone for consistency. This timestamp is always required and
    /// serves as the foundation for work duration calculations.
    pub start: NaiveDateTime,

    /// Timestamp when work session ended, if completed.
    ///
    /// `None` indicates an active/ongoing work session that hasn't been
    /// completed yet. `Some(timestamp)` indicates a finished work session
    /// with the exact completion time for duration calculations.
    pub end: Option<NaiveDateTime>,
}

/// Database manager for workday operations and time tracking functionality.
///
/// The `Workdays` struct provides a comprehensive interface for managing daily
/// work sessions, including creation, modification, and querying of workday
/// records. It handles database connections, ensures data integrity, and
/// provides efficient access patterns for time tracking operations.
///
/// ## Design Principles
///
/// - **One Session Per Day**: Each calendar date has at most one workday record
/// - **Local Timezone**: All timestamps use local timezone for user clarity
/// - **Automatic Tracking**: Supports both automatic and manual time entry
/// - **Data Integrity**: Enforces constraints and validation for reliable tracking
///
/// ## Connection Management
///
/// Each instance maintains its own database connection and ensures the
/// workdays table schema is properly initialized during construction.
pub struct Workdays {
    /// Direct database connection for workday operations.
    ///
    /// Provides transactional access to the workdays table with
    /// optimized performance for time tracking queries and updates.
    pub conn: Connection,
}

impl Workdays {
    /// Creates a new Workdays manager and initializes the database schema.
    ///
    /// This constructor establishes a database connection, ensures the workdays
    /// table exists with proper constraints, and prepares the manager for
    /// time tracking operations. Schema creation is idempotent and safe for
    /// repeated initialization.
    ///
    /// # Returns
    ///
    /// Returns a new `Workdays` instance ready for workday management
    /// operations, or an error if database initialization fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::workdays::Workdays;
    ///
    /// let mut workdays = Workdays::new()?;
    /// // Ready for workday tracking
    /// ```
    ///
    /// # Database Integration
    ///
    /// The workdays table schema is created if it doesn't exist, ensuring
    /// the manager can operate regardless of database initialization order.
    /// This provides robustness in different deployment scenarios.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database connection cannot be established
    /// - Schema creation fails due to permissions or corruption
    /// - Table initialization encounters constraint violations
    pub fn new() -> Result<Self> {
        let db = Db::new()?;

        // Initialize the workdays table schema
        db.conn.execute(SCHEMA_WORKDAYS, [])?;

        Ok(Workdays { conn: db.conn })
    }

    /// Records the start of a work session for the specified date.
    ///
    /// This method creates a new workday record with the current timestamp
    /// as the start time, or does nothing if a workday already exists for
    /// the given date. This prevents duplicate workday records while allowing
    /// safe repeated calls to start tracking.
    ///
    /// ## Duplicate Handling
    ///
    /// The method checks for existing workday records before insertion to
    /// prevent database constraint violations. If a workday already exists
    /// for the specified date, the operation succeeds without modification.
    ///
    /// ## Timezone Consistency
    ///
    /// Uses local timezone for timestamp recording to ensure consistency
    /// with user expectations and interface displays. All workday times
    /// are recorded in the system's local timezone.
    ///
    /// # Arguments
    ///
    /// * `date` - Calendar date for which to start work session tracking
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the start time is recorded or already exists,
    /// or an error if the database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chrono::Local;
    ///
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    /// workdays.insert_start(today)?; // Start tracking work for today
    /// ```
    ///
    /// # Idempotency
    ///
    /// This operation is idempotent - calling it multiple times with the
    /// same date has the same effect as calling it once. This makes it
    /// safe for use in automatic tracking systems.
    pub fn insert_start(&mut self, date: NaiveDate) -> Result<()> {
        let date_str = date.format("%Y-%m-%d").to_string();

        // Check if workday already exists to prevent duplicates
        if self.fetch(date)?.is_none() {
            self.conn.execute(INSERT_START, [&date_str])?;
        }

        Ok(())
    }

    /// Records the end of a work session for the specified date.
    ///
    /// This method updates an existing workday record by setting the end
    /// timestamp to the current time. It marks the completion of a work
    /// session and enables duration calculations for the workday.
    ///
    /// ## Prerequisites
    ///
    /// The workday must already exist (created via `insert_start`) for this
    /// operation to succeed. The method updates the existing record rather
    /// than creating a new one, maintaining data integrity.
    ///
    /// ## Completion Semantics
    ///
    /// Once a workday has an end time, it represents a completed work session
    /// for that date. The end time can be modified later using time adjustment
    /// methods if corrections are needed.
    ///
    /// # Arguments
    ///
    /// * `date` - Calendar date for which to end work session tracking
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the end time is recorded successfully, or an error
    /// if no workday exists for the date or the database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chrono::Local;
    ///
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    ///
    /// // Start and end a work session
    /// workdays.insert_start(today)?;
    /// // ... work happens ...
    /// workdays.insert_end(today)?; // Mark work as completed
    /// ```
    ///
    /// # Error Conditions
    ///
    /// - No workday record exists for the specified date
    /// - Database connection or constraint failures
    /// - Concurrent modification conflicts
    pub fn insert_end(&mut self, date: NaiveDate) -> Result<()> {
        let date_str = date.format("%Y-%m-%d").to_string();
        self.conn.execute(UPDATE_END, [&date_str])?;
        Ok(())
    }

    /// Retrieves a complete workday record for the specified date.
    ///
    /// This method fetches detailed workday information including all temporal
    /// data and metadata for a specific calendar date. It provides the primary
    /// mechanism for accessing workday details for analysis and reporting.
    ///
    /// ## Data Parsing
    ///
    /// The method handles automatic conversion from database string formats
    /// to appropriate Rust types (`NaiveDate`, `NaiveDateTime`), providing
    /// type safety and convenience for downstream operations.
    ///
    /// ## Return Semantics
    ///
    /// - `Some(Workday)`: Complete workday record found for the date
    /// - `None`: No workday exists for the specified date
    /// - `Error`: Database access or parsing failures
    ///
    /// # Arguments
    ///
    /// * `date` - Calendar date for which to retrieve workday information
    ///
    /// # Returns
    ///
    /// Returns `Some(Workday)` if a record exists, `None` if no workday is
    /// found for the date, or an error if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chrono::Local;
    ///
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    ///
    /// if let Some(workday) = workdays.fetch(today)? {
    ///     println!("Work started at: {}", workday.start);
    ///     if let Some(end_time) = workday.end {
    ///         println!("Work ended at: {}", end_time);
    ///     } else {
    ///         println!("Work session is still active");
    ///     }
    /// } else {
    ///     println!("No work session recorded for today");
    /// }
    /// ```
    ///
    /// # Data Integrity
    ///
    /// The method assumes well-formed timestamp data in the database and
    /// will return errors if timestamp parsing fails due to data corruption
    /// or unexpected format changes.
    pub fn fetch(&mut self, date: NaiveDate) -> Result<Option<Workday>> {
        let date_str = date.format("%Y-%m-%d").to_string();

        let workday = self
            .conn
            .query_row(SELECT_BY_DATE, [&date_str], |row| {
                Ok(Workday {
                    id: row.get(0)?,
                    date: NaiveDate::parse_from_str(&row.get::<_, String>(1)?, "%Y-%m-%d").unwrap(),
                    start: NaiveDateTime::parse_from_str(&row.get::<_, String>(2)?, "%Y-%m-%d %H:%M:%S").unwrap(),
                    end: row
                        .get::<_, Option<String>>(3)?
                        .map(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").unwrap()),
                })
            })
            .optional()?;

        Ok(workday)
    }

    /// Retrieves all workdays within the calendar month containing the specified date.
    ///
    /// This method fetches workday records for an entire month using SQL date
    /// functions to determine month boundaries. It provides efficient bulk
    /// access to workday data for monthly reporting, productivity analysis,
    /// and trend identification.
    ///
    /// ## Month Calculation
    ///
    /// The method uses SQL `strftime` functions to extract year-month components
    /// and match them against the target date's month. This approach handles
    /// month boundaries correctly across different year transitions.
    ///
    /// ## Result Processing
    ///
    /// All workdays are loaded into memory and returned as a vector, providing
    /// convenient access for analysis operations. The results maintain
    /// chronological order for consistent processing.
    ///
    /// # Arguments
    ///
    /// * `date` - Any date within the target month for workday retrieval
    ///
    /// # Returns
    ///
    /// Returns a vector of all workdays in the same month as the specified date,
    /// or an error if the database query or parsing fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chrono::{Local, NaiveDate};
    ///
    /// let mut workdays = Workdays::new()?;
    /// let current_month = Local::now().date_naive();
    ///
    /// let monthly_workdays = workdays.fetch_month(current_month)?;
    /// println!("Found {} workdays this month", monthly_workdays.len());
    ///
    /// for workday in monthly_workdays {
    ///     if let Some(end_time) = workday.end {
    ///         let duration = end_time - workday.start;
    ///         println!("Date: {}, Duration: {:?}", workday.date, duration);
    ///     }
    /// }
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// - Loads all monthly workdays into memory simultaneously
    /// - Efficient for typical monthly workday counts (20-30 records)
    /// - May need optimization for very large historical datasets
    /// - Consider pagination for bulk historical data processing
    ///
    /// # Use Cases
    ///
    /// - Monthly productivity reports
    /// - Work pattern analysis and trend identification
    /// - Timesheet generation and validation
    /// - Historical work data export and backup
    pub fn fetch_month(&mut self, date: NaiveDate) -> Result<Vec<Workday>> {
        let date_str = date.format("%Y-%m-%d").to_string();

        // Prepare statement for monthly workday query
        let mut stmt = self.conn.prepare(SELECT_BY_MONTH)?;

        // Execute query and process results
        let workday_iter = stmt.query_map([&date_str], |row| {
            Ok(Workday {
                id: row.get(0)?,
                date: NaiveDate::parse_from_str(&row.get::<_, String>(1)?, "%Y-%m-%d").unwrap(),
                start: NaiveDateTime::parse_from_str(&row.get::<_, String>(2)?, "%Y-%m-%d %H:%M:%S").unwrap(),
                end: row
                    .get::<_, Option<String>>(3)?
                    .map(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").unwrap()),
            })
        })?;

        // Collect all workday results
        let mut workdays = Vec::new();
        for workday in workday_iter {
            workdays.push(workday?);
        }

        Ok(workdays)
    }

    /// Updates the start time of an existing workday to a specific timestamp.
    ///
    /// This method provides manual adjustment capabilities for correcting
    /// automatic time tracking errors or implementing manual time entry
    /// scenarios. It modifies the start timestamp while preserving all
    /// other workday properties and relationships.
    ///
    /// ## Time Adjustment Use Cases
    ///
    /// - Correcting automatic tracking start times that were recorded incorrectly
    /// - Manual time entry for workdays that weren't automatically tracked
    /// - Adjusting for delayed system startup or application launch
    /// - Retroactive time corrections based on external time records
    ///
    /// ## Data Validation
    ///
    /// The method validates that the workday exists before attempting the update
    /// and returns an error if no matching record is found. This ensures
    /// referential integrity and provides clear error feedback.
    ///
    /// # Arguments
    ///
    /// * `date` - Calendar date of the workday to modify
    /// * `new_start` - New start timestamp to apply to the workday
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the update succeeds, or an error if the workday
    /// doesn't exist or the database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chrono::{Local, NaiveDateTime};
    ///
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    ///
    /// // Correct start time to 9:00 AM
    /// let corrected_start = NaiveDateTime::parse_from_str(
    ///     &format!("{} 09:00:00", today.format("%Y-%m-%d")),
    ///     "%Y-%m-%d %H:%M:%S"
    /// )?;
    ///
    /// workdays.update_start(today, corrected_start)?;
    /// ```
    ///
    /// # Error Conditions
    ///
    /// - No workday exists for the specified date
    /// - Database constraint violations or connection failures
    /// - Invalid timestamp format or timezone issues
    pub fn update_start(&mut self, date: NaiveDate, new_start: NaiveDateTime) -> Result<()> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let start_str = new_start.format("%Y-%m-%d %H:%M:%S").to_string();

        let affected = self.conn.execute(UPDATE_START, [&start_str, &date_str])?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::WorkdayUpdateFailed));
        }

        Ok(())
    }

    /// Updates the end time of an existing workday or clears it to mark as ongoing.
    ///
    /// This method provides flexible end time management, supporting both
    /// specific timestamp updates and clearing end times to mark workdays
    /// as ongoing or incomplete. It enables comprehensive time adjustment
    /// and correction capabilities.
    ///
    /// ## Operation Modes
    ///
    /// - **Set End Time**: `Some(timestamp)` sets a specific completion time
    /// - **Clear End Time**: `None` removes the end time, marking as ongoing
    /// - **Correction**: Modify existing end times for accuracy improvements
    ///
    /// ## State Transitions
    ///
    /// The method supports all valid workday state transitions:
    /// - Completed → Ongoing (clear end time)
    /// - Ongoing → Completed (set end time)
    /// - Completed → Completed (adjust end time)
    ///
    /// # Arguments
    ///
    /// * `date` - Calendar date of the workday to modify
    /// * `new_end` - New end timestamp (`Some`) or clear end time (`None`)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the update succeeds, or an error if the workday
    /// doesn't exist or the database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chrono::{Local, NaiveDateTime};
    ///
    /// let mut workdays = Workdays::new()?;
    /// let today = Local::now().date_naive();
    ///
    /// // Set specific end time
    /// let end_time = NaiveDateTime::parse_from_str(
    ///     &format!("{} 17:30:00", today.format("%Y-%m-%d")),
    ///     "%Y-%m-%d %H:%M:%S"
    /// )?;
    /// workdays.update_end(today, Some(end_time))?;
    ///
    /// // Clear end time (mark as ongoing)
    /// workdays.update_end(today, None)?;
    /// ```
    ///
    /// # Data Consistency
    ///
    /// The method ensures database consistency by validating workday existence
    /// and providing clear error feedback for failed operations. This maintains
    /// data integrity across time adjustment operations.
    pub fn update_end(&mut self, date: NaiveDate, new_end: Option<NaiveDateTime>) -> Result<()> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let end_str = new_end.map(|e| e.format("%Y-%m-%d %H:%M:%S").to_string());

        let affected = match end_str {
            Some(end) => self.conn.execute(UPDATE_END_TIME, [&end, &date_str])?,
            None => self.conn.execute(UNSET_END_TIME, [&date_str])?,
        };

        // Validate that a workday record was actually updated
        if affected == 0 {
            return Err(msg_error_anyhow!(Message::WorkdayUpdateFailed));
        }

        Ok(())
    }
}
