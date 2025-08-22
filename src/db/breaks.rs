//! Manual break periods management for productivity optimization.
//!
//! This module handles user-defined break periods that complement automatic pause detection.
//! Manual breaks allow users to improve their productivity metrics by marking specific
//! time periods as intentional breaks.
//!
//! ## Features
//!
//! - **Manual Break Creation**: Add intentional break periods to improve productivity
//! - **Smart Placement**: Avoid conflicts with existing pauses and maintain minimum work intervals
//! - **Daily Management**: Retrieve and manage breaks for specific dates
//! - **Productivity Integration**: Seamlessly integrate with productivity calculations

use crate::db::db::Db;
use anyhow::Result;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use rusqlite::params;

/// Represents a manually added break period.
///
/// Manual breaks are user-defined periods that complement automatic pause detection
/// to provide more accurate productivity calculations. Unlike automatic pauses,
/// breaks are intentionally added to represent planned rest periods.
#[derive(Debug, Clone)]
pub struct Break {
    /// Unique identifier for the break record
    pub id: Option<i64>,
    
    /// Date this break belongs to
    pub date: NaiveDate,
    
    /// Start time of the break
    pub start: NaiveDateTime,
    
    /// End time of the break
    pub end: NaiveDateTime,
    
    /// Duration of the break in minutes
    pub duration: Duration,
    
    /// Optional reason for the break
    pub reason: Option<String>,
    
    /// When this break record was created
    pub created_at: Option<NaiveDateTime>,
}

/// Database operations for manual break management.
///
/// Provides CRUD operations for break records with proper error handling
/// and integration with the existing database infrastructure.
pub struct Breaks {
    db: Db,
}

impl Breaks {
    /// Create a new Breaks database manager.
    ///
    /// Initializes the breaks manager with database connection and ensures
    /// the breaks table exists through the migration system.
    ///
    /// # Returns
    ///
    /// Returns a configured Breaks instance ready for database operations.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let breaks_db = Breaks::new()?;
    /// ```
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        Ok(Self { db })
    }

    /// Insert a new break record into the database.
    ///
    /// Creates a new break record with the provided information. The break
    /// duration is calculated and stored automatically based on start and end times.
    ///
    /// # Arguments
    ///
    /// * `break_record` - The break information to insert
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful insertion, or an error if the database
    /// operation fails or validation errors occur.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let break_record = Break {
    ///     id: None,
    ///     date: date,
    ///     start: start_time,
    ///     end: end_time,
    ///     duration: Duration::minutes(30),
    ///     reason: Some("Lunch break".to_string()),
    ///     created_at: None,
    /// };
    /// 
    /// breaks_db.insert(&break_record)?;
    /// ```
    pub fn insert(&self, break_record: &Break) -> Result<i64> {
        let conn = &self.db.conn;
        
        let _result = conn.execute(
            "INSERT INTO breaks (date, start_time, end_time, duration, reason, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            params![
                break_record.date,
                break_record.start,
                break_record.end,
                break_record.duration.num_minutes(),
                break_record.reason,
            ],
        )?;
        
        Ok(conn.last_insert_rowid())
    }

    /// Retrieve all breaks for a specific date.
    ///
    /// Returns all manual break records for the given date, ordered by start time.
    /// This is typically used for daily productivity calculations and reporting.
    ///
    /// # Arguments
    ///
    /// * `date` - The date to retrieve breaks for
    ///
    /// # Returns
    ///
    /// Returns a vector of Break records for the specified date, or an error
    /// if the database query fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let today = chrono::Local::now().date_naive();
    /// let breaks = breaks_db.get_daily_breaks(today)?;
    /// ```
    pub fn get_daily_breaks(&self, date: NaiveDate) -> Result<Vec<Break>> {
        let conn = &self.db.conn;
        
        let mut stmt = conn.prepare(
            "SELECT id, date, start_time, end_time, duration, reason, created_at 
             FROM breaks 
             WHERE date = ?1 
             ORDER BY start_time"
        )?;
        
        let break_iter = stmt.query_map(params![date], |row| {
            Ok(Break {
                id: Some(row.get(0)?),
                date: row.get(1)?,
                start: row.get(2)?,
                end: row.get(3)?,
                duration: Duration::minutes(row.get::<_, i64>(4)?),
                reason: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        
        let mut breaks = Vec::new();
        for break_result in break_iter {
            breaks.push(break_result?);
        }
        
        Ok(breaks)
    }

    /// Delete a break record by ID.
    ///
    /// Removes a break record from the database. This is typically used
    /// for correcting mistakes or adjusting productivity calculations.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the break record to delete
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful deletion, or an error if the record
    /// doesn't exist or the database operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// breaks_db.delete(break_id)?;
    /// ```
    pub fn delete(&self, id: i64) -> Result<()> {
        let conn = &self.db.conn;
        
        let affected_rows = conn.execute("DELETE FROM breaks WHERE id = ?1", params![id])?;
        
        if affected_rows == 0 {
            return Err(anyhow::anyhow!("Break record with ID {} not found", id));
        }
        
        Ok(())
    }

    /// Get a specific break by ID.
    ///
    /// Retrieves a single break record by its unique identifier.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the break record to retrieve
    ///
    /// # Returns
    ///
    /// Returns `Some(Break)` if found, `None` if not found, or an error
    /// if the database query fails.
    pub fn get_by_id(&self, id: i64) -> Result<Option<Break>> {
        let conn = &self.db.conn;
        
        let mut stmt = conn.prepare(
            "SELECT id, date, start_time, end_time, duration, reason, created_at 
             FROM breaks 
             WHERE id = ?1"
        )?;
        
        let mut break_iter = stmt.query_map(params![id], |row| {
            Ok(Break {
                id: Some(row.get(0)?),
                date: row.get(1)?,
                start: row.get(2)?,
                end: row.get(3)?,
                duration: Duration::minutes(row.get::<_, i64>(4)?),
                reason: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        
        match break_iter.next() {
            Some(break_result) => Ok(Some(break_result?)),
            None => Ok(None),
        }
    }
}