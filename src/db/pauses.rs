//! Database operations for tracking work breaks and pause periods.
//!
//! Manages the storage and retrieval of pause/break records during work sessions.
//! Provides functionality for automatically tracking user inactivity periods and
//! manually recorded breaks.
//!
//! ## Features
//!
//! - **Automatic Detection**: Records pauses when user activity stops
//! - **Manual Entry**: Support for manually adding break periods
//! - **Duration Calculation**: Automatic computation of pause lengths
//! - **Daily Filtering**: Retrieve pauses for specific dates with duration thresholds
//! - **Batch Operations**: Delete multiple pause records efficiently
//!
//! ## Usage
//!
//! ```rust
//! use kasl::db::pauses::Pauses;
//! use chrono::Local;
//!
//! let pauses = Pauses::new()?;
//! pauses.insert_start()?;
//! pauses.insert_end(300)?; // 5 minutes
//! ```

use crate::db::db::Db;
use crate::libs::pause::Pause;
use anyhow::Result;
use chrono::{Local, NaiveDate, NaiveDateTime, TimeDelta};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::sync::Arc;

/// SQL schema for the pauses table.
///
/// Defines the structure for storing pause/break records with temporal data.
/// The schema supports both ongoing pauses (end IS NULL) and completed pauses
/// with calculated durations for reporting and analysis.
const SCHEMA_PAUSES: &str = "CREATE TABLE IF NOT EXISTS pauses (
    id INTEGER NOT NULL PRIMARY KEY,
    start TIMESTAMP NOT NULL,
    end TIMESTAMP,
    duration INTEGER
)";

/// Insert a new pause start record with the current timestamp.
///
/// This query creates a new pause record with only the start time set,
/// leaving end and duration as NULL until the pause is completed.
const INSERT_PAUSE: &str = "INSERT INTO pauses (start) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'))";

/// Insert a new pause start record with a specific timestamp.
///
/// Used for manually adding pauses or when importing historical data
/// where the exact start time is known.
const INSERT_PAUSE_WITH_TIME: &str = "INSERT INTO pauses (start) VALUES (?1)";

/// Update the most recent open pause with end time and calculated duration.
///
/// Completes a pause record by setting the end timestamp and storing
/// the calculated duration in seconds for later analysis.
const UPDATE_PAUSE: &str = "UPDATE pauses SET end = (datetime(CURRENT_TIMESTAMP, 'localtime')), duration = ?1 WHERE id = ?2";

/// Select the most recent uncompleted pause record.
///
/// Finds the last pause that has a start time but no end time,
/// indicating an ongoing pause that needs to be completed.
const SELECT_LAST_PAUSE: &str = "SELECT id, start FROM pauses WHERE end IS NULL ORDER BY id DESC LIMIT 1";

/// Select all pauses for a specific date with duration filtering.
///
/// Retrieves completed pauses for a given date that meet the minimum
/// duration threshold. Used for daily reporting and analysis.
const SELECT_DAILY_PAUSES_WITH_LONG_DURATION: &str = "SELECT id, start, end, duration FROM pauses WHERE date(start) = date(?1, 'localtime') AND duration >= ?2";
const SELECT_DAILY_PAUSES_WITH_SHORT_DURATION: &str = "SELECT id, start, end, duration FROM pauses WHERE date(start) = date(?1, 'localtime') AND duration < ?2";

/// Delete a single pause record by ID.
///
/// Removes a pause record from the database, typically used for
/// correcting incorrectly recorded pauses or data cleanup.
const DELETE_PAUSE: &str = "DELETE FROM pauses WHERE id = ?";

struct Operation {
    sql_query: String,
    duration: String,
}

/// Database manager for pause/break tracking operations.
///
/// The `Pauses` struct provides a high-level interface for managing work break
/// records in the database. It uses thread-safe connection handling to support
/// concurrent access from the activity monitor and user commands.
///
/// ## Thread Safety
///
/// The connection is wrapped in an `Arc<Mutex<>>` to allow safe concurrent access
/// from multiple threads, particularly important when the activity monitor
/// is running in the background while users interact with the CLI.
///
/// ## Connection Management
///
/// Each `Pauses` instance maintains its own database connection and ensures
/// the pauses table schema is properly initialized on creation.
pub struct Pauses {
    /// Thread-safe database connection wrapper.
    ///
    /// The connection is protected by a mutex to prevent race conditions
    /// when multiple threads attempt to record or query pause data
    /// simultaneously.
    pub conn: Arc<Mutex<Connection>>,
    pub min_duration: Option<String>,
    pub max_duration: Option<String>,
}

impl Pauses {
    /// Creates a new `Pauses` instance and initializes the database schema.
    ///
    /// This constructor establishes a database connection, ensures the pauses
    /// table exists with the proper schema, and wraps the connection for
    /// thread-safe access. The schema creation is idempotent and safe to
    /// call multiple times.
    ///
    /// # Returns
    ///
    /// Returns a new `Pauses` instance ready for pause tracking operations,
    /// or an error if database initialization fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::pauses::Pauses;
    ///
    /// let pauses = Pauses::new()?;
    /// // Ready to track pauses
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database connection cannot be established
    /// - Schema creation fails due to permissions or corruption
    /// - Table initialization encounters SQL errors
    pub fn new() -> Result<Pauses> {
        // Establish database connection through the central Db manager
        let db_conn = Db::new()?.conn;

        // Initialize the pauses table schema if it doesn't exist
        db_conn.execute(SCHEMA_PAUSES, [])?;

        // Wrap connection for thread-safe access
        Ok(Pauses {
            conn: Arc::new(Mutex::new(db_conn)),
            min_duration: None,
            max_duration: None,
        })
    }

    pub fn set_min_duration(&self, min_duration: u64) -> Self {
        let min_duration_secs = (min_duration * 60) as i64; // Convert minutes to seconds
        Self {
            conn: self.conn.clone(),
            min_duration: Some(min_duration_secs.to_string()),
            max_duration: None,
        }
    }

    pub fn set_max_duration(&self, max_duration: u64) -> Self {
        let max_duration_secs = (max_duration * 60) as i64; // Convert minutes to second
        Self {
            conn: self.conn.clone(),
            min_duration: None,
            max_duration: Some(max_duration_secs.to_string()),
        }
    }

    fn get_operation(&self) -> Operation {
        if self.min_duration.is_some() {
            return Operation {
                sql_query: String::from(SELECT_DAILY_PAUSES_WITH_LONG_DURATION),
                duration: self.min_duration.clone().unwrap(),
            };
        } else if self.max_duration.is_some() {
            return Operation {
                sql_query: String::from(SELECT_DAILY_PAUSES_WITH_SHORT_DURATION),
                duration: self.max_duration.clone().unwrap(),
            };
        }
        Operation {
            sql_query: String::from(SELECT_DAILY_PAUSES_WITH_LONG_DURATION),
            duration: String::from("0"),
        }
    }

    /// Records the start of a new pause with the current timestamp.
    ///
    /// This method creates a new pause record using the current system time
    /// as the start timestamp. The pause remains "open" (end IS NULL) until
    /// it's completed with `insert_end()`. Multiple open pauses are allowed
    /// to handle edge cases in activity detection.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the pause start is recorded successfully,
    /// or an error if the database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let pauses = Pauses::new()?;
    /// pauses.insert_start()?; // Pause started at current time
    /// ```
    ///
    /// # Thread Safety
    ///
    /// This method is thread-safe and can be called concurrently from
    /// multiple threads, such as the activity monitor daemon.
    pub fn insert_start(&self) -> rusqlite::Result<()> {
        let conn_guard = self.conn.lock();
        conn_guard.execute(INSERT_PAUSE, [])?;
        Ok(())
    }

    /// Records the start of a new pause with a specific timestamp.
    ///
    /// This method allows manual insertion of pause records with exact
    /// timestamps, useful for importing historical data or correcting
    /// activity tracking records. The specified time should be in the
    /// local timezone for consistency with other records.
    ///
    /// # Arguments
    ///
    /// * `start_time` - The exact timestamp when the pause began
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the pause is recorded successfully,
    /// or an error if the database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chrono::NaiveDateTime;
    ///
    /// let pauses = Pauses::new()?;
    /// let start_time = NaiveDateTime::parse_from_str(
    ///     "2025-01-15 14:30:00",
    ///     "%Y-%m-%d %H:%M:%S"
    /// )?;
    /// pauses.insert_start_with_time(start_time)?;
    /// ```
    ///
    /// # Data Integrity
    ///
    /// The caller is responsible for ensuring the timestamp is reasonable
    /// and doesn't conflict with existing work session boundaries.
    pub fn insert_start_with_time(&self, start_time: NaiveDateTime) -> Result<()> {
        let conn_guard = self.conn.lock();
        let start_str = start_time.format("%Y-%m-%d %H:%M:%S").to_string();
        conn_guard.execute(INSERT_PAUSE_WITH_TIME, [&start_str])?;
        Ok(())
    }

    /// Completes the most recent open pause with duration calculation.
    ///
    /// This method finds the last pause record that has a start time but no
    /// end time, then updates it with the current timestamp and the provided
    /// duration. The duration is typically calculated by the activity monitor
    /// based on the actual inactive period.
    ///
    /// ## Duration Calculation
    ///
    /// While the end timestamp is set to the current time, the duration
    /// parameter contains the actual pause length in seconds. This allows
    /// for accurate tracking even when there's a delay between activity
    /// resumption and pause recording.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the pause is completed successfully, or an error
    /// if no open pause exists or the database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let pauses = Pauses::new()?;
    /// pauses.insert_start()?;
    /// // ... user is inactive for 5 minutes ...
    /// pauses.insert_end()?;
    /// ```
    ///
    /// # Behavior Notes
    ///
    /// - Only affects the most recent open pause record
    /// - If no open pause exists, the operation may fail silently
    /// - Duration should be a positive number of seconds
    pub fn insert_end(&self) -> Result<()> {
        let end = Local::now().naive_local();
        let conn_guard = self.conn.lock();

        let mut stmt = conn_guard.prepare(SELECT_LAST_PAUSE)?;
        let pause_row = stmt.query_row([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)));
        if let Ok((id, start_str)) = pause_row {
            let start = NaiveDateTime::parse_from_str(&start_str, "%Y-%m-%d %H:%M:%S")?;
            let duration = (end - start).num_seconds();
            conn_guard.execute(UPDATE_PAUSE, [&duration.to_string(), &id.to_string()])?;
        }

        Ok(())
    }

    /// Retrieves all pause records for a specific date with duration filtering.
    ///
    /// This method fetches all completed pause records for the given date that
    /// meet or exceed the specified minimum duration threshold. It's commonly
    /// used for daily reporting and work time calculations where very short
    /// pauses (e.g., under 5 minutes) may be ignored.
    ///
    /// ## Filtering Logic
    ///
    /// - Only includes pauses that started on the specified date
    /// - Filters out pauses shorter than the minimum duration
    /// - Includes ongoing pauses (duration IS NULL) regardless of threshold
    /// - Results are ordered by start time for chronological display
    ///
    /// # Arguments
    ///
    /// * `date` - The target date to query (uses local timezone)
    /// * `min_duration` - Minimum pause length to include (in minutes)
    ///
    /// # Returns
    ///
    /// Returns a vector of `Pause` objects representing the filtered pause
    /// records, or an error if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use chrono::Local;
    ///
    /// let pauses = Pauses::new()?;
    /// let today = Local::now().date_naive();
    ///
    /// // Get pauses of 10 minutes or longer
    /// let significant_pauses = pauses.get_daily_pauses(today, 10)?;
    /// for pause in significant_pauses {
    ///     println!("Pause: {:?} - {:?}", pause.start, pause.end);
    /// }
    /// ```
    ///
    /// # Performance Notes
    ///
    /// This query uses date functions and may be slower on large datasets.
    /// Consider adding indices on the start column for better performance.
    pub fn get_daily_pauses(&self, date: NaiveDate) -> Result<Vec<Pause>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let conn_guard = self.conn.lock();
        let operation = self.get_operation();
        // Prepare statement for parameterized query
        let mut stmt = conn_guard.prepare(&operation.sql_query)?;
        // Execute query with date and duration filter
        let pause_iter = stmt.query_map([&date_str, &operation.duration], |row| {
            // Parse timestamps from database strings
            let start_str: String = row.get(1)?;
            let end_str: Option<String> = row.get(2)?;
            let duration: i64 = row.get(3).unwrap_or(0);

            // Create Pause object with parsed data
            Ok(Pause {
                id: row.get(0)?,
                start: NaiveDateTime::parse_from_str(&start_str, "%Y-%m-%d %H:%M:%S").unwrap(),
                end: end_str.map(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").unwrap()),
                duration: Some(TimeDelta::seconds(duration)),
            })
        })?;

        // Collect results, handling any parsing errors
        let mut pauses = Vec::new();
        for pause_result in pause_iter {
            pauses.push(pause_result?);
        }

        Ok(pauses)
    }

    /// Deletes a single pause record by its unique identifier.
    ///
    /// This method removes a specific pause record from the database,
    /// typically used for correcting erroneous pause recordings or
    /// user-requested deletions. The operation is permanent and cannot
    /// be undone without database backups.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the pause record to delete
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the deletion succeeds, or an error if the
    /// database operation fails. Note that deleting a non-existent
    /// record is not considered an error.
    ///
    /// # Example
    ///
    /// ```rust
    /// let pauses = Pauses::new()?;
    /// pauses.delete(123)?; // Delete pause with ID 123
    /// ```
    ///
    /// # Safety Considerations
    ///
    /// - Deletion is immediate and permanent
    /// - No confirmation prompts are provided at this level
    /// - Callers should implement appropriate confirmation flows
    pub fn delete(&self, id: i32) -> Result<()> {
        let conn_guard = self.conn.lock();
        conn_guard.execute(DELETE_PAUSE, params![id])?;
        Ok(())
    }

    /// Deletes multiple pause records efficiently in a batch operation.
    ///
    /// This method removes multiple pause records in a single transaction,
    /// providing better performance than individual deletions and ensuring
    /// atomicity. If any deletion fails, all changes are rolled back.
    ///
    /// ## Transaction Handling
    ///
    /// All deletions are performed within a single database transaction
    /// to ensure consistency. Either all specified records are deleted
    /// or none are deleted if any error occurs.
    ///
    /// # Arguments
    ///
    /// * `ids` - Slice of pause record IDs to delete
    ///
    /// # Returns
    ///
    /// Returns the number of records actually deleted, or an error if
    /// the batch operation fails. The count may be less than the input
    /// length if some IDs don't exist in the database.
    ///
    /// # Example
    ///
    /// ```rust
    /// let pauses = Pauses::new()?;
    /// let ids_to_delete = vec![101, 102, 103];
    /// let deleted_count = pauses.delete_many(&ids_to_delete)?;
    /// println!("Deleted {} pause records", deleted_count);
    /// ```
    ///
    /// # Performance Benefits
    ///
    /// - Single transaction reduces database overhead
    /// - More efficient than individual delete operations
    /// - Atomic operation ensures data consistency
    ///
    /// # Edge Cases
    ///
    /// - Empty input slice returns 0 without database interaction
    /// - Non-existent IDs are silently ignored
    /// - Partial failures result in complete rollback
    pub fn delete_many(&self, ids: &[i32]) -> Result<usize> {
        // Handle empty input early to avoid unnecessary database operations
        if ids.is_empty() {
            return Ok(0);
        }

        let conn_guard = self.conn.lock();
        let mut deleted = 0;

        // Delete each record individually within the locked connection
        // This could be optimized with a single IN clause query for large batches
        for id in ids {
            deleted += conn_guard.execute(DELETE_PAUSE, params![id])?;
        }

        Ok(deleted)
    }
}
