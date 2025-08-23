//! Productivity calculation utilities for work time analysis.
//!
//! This module provides centralized productivity calculations that properly handle
//! different types of work interruptions (pauses vs. breaks) to give accurate
//! productivity metrics.
//!
//! ## Key Concepts
//!
//! - **Short Pauses**: Brief interruptions that are not recorded in the database (< min_pause_duration)
//! - **Long Pauses**: Extended interruptions that are recorded as pause records (>= min_pause_duration)  
//! - **Manual Breaks**: User-defined break periods that are excluded from productivity calculations
//!
//! ## Productivity Formula
//!
//! ```text
//! Productivity = (Net Work Time / Available Work Time) * 100
//!
//! Where:
//! - Net Work Time = Total Time - Long Pauses - Manual Breaks
//! - Available Work Time = Total Time - Manual Breaks
//! ```

use crate::db::breaks::{Break, Breaks};
use crate::db::pauses::Pauses;
use crate::db::workdays::Workday;
use crate::libs::config::{Config, ProductivityConfig};
use crate::libs::pause::Pause;
use anyhow::Result;
use chrono::Duration;

/// Productivity calculator with comprehensive work time analysis.
///
/// This structure holds all the data needed for accurate productivity calculations,
/// including workday timing, manual breaks, different categories of pauses, and
/// configuration settings. It provides the central calculation logic used throughout
/// the application.
///
/// ## Data Categories
///
/// - **Workday**: Start/end times defining the total work session
/// - **Breaks**: Manual breaks explicitly added by the user  
/// - **Short Pauses**: Automatic pauses below the minimum threshold (not stored in DB)
/// - **Long Pauses**: Automatic pauses above the minimum threshold (stored in DB)
/// - **Config**: Productivity configuration settings and thresholds
///
/// ## Usage Pattern
///
/// 1. Create instance with `Productivity::new()` - automatically loads all relevant data
/// 2. Call `calculate_productivity()` for the main productivity percentage
/// 3. Use helper methods for break recommendations and analysis (now parameter-free)
pub struct Productivity {
    /// The workday record containing start/end times
    pub workday: Workday,
    /// Manual breaks explicitly added by the user
    pub breaks: Vec<Break>,
    /// Short automatic pauses (< min_pause_duration, not in database)
    pub short_pauses: Vec<Pause>,
    /// Long automatic pauses (>= min_pause_duration, stored in database)
    pub long_pauses: Vec<Pause>,
    /// Productivity configuration settings and thresholds
    pub config: ProductivityConfig,
}

impl Productivity {
    /// Creates a new productivity calculator for the given workday.
    ///
    /// This constructor automatically loads all relevant data for productivity calculations:
    /// - Reads the current configuration to get pause duration thresholds
    /// - Loads manual breaks from the database for the workday date
    /// - Loads short pauses (below min_pause_duration threshold)  
    /// - Loads long pauses (at or above min_pause_duration threshold)
    ///
    /// The pause categorization is based on the `min_pause_duration` setting from
    /// the monitor configuration. This threshold determines which pauses are stored
    /// in the database vs. calculated on-the-fly.
    ///
    /// # Arguments
    ///
    /// * `workday` - The workday record to analyze
    ///
    /// # Returns
    ///
    /// Returns a configured `Productivity` instance with all data loaded.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Configuration file cannot be read
    /// - Database queries fail
    /// - Data integrity issues are encountered
    ///
    /// # Examples
    ///
    /// ```rust
    /// let productivity = Productivity::new(&workday)?;
    /// let current_productivity = productivity.calculate_productivity();
    /// println!("Current productivity: {:.1}%", current_productivity);
    /// ```
    pub fn new(workday: &Workday) -> Result<Self> {
        let config = Config::read()?;
        let monitor_config = config.monitor.unwrap_or_default();
        let productivity_config = config.productivity.unwrap_or_default();

        Ok(Self {
            workday: workday.clone(),
            breaks: Breaks::new()?.get_daily_breaks(workday.date)?,
            short_pauses: Pauses::new()?
                .set_max_duration(monitor_config.min_pause_duration)
                .get_daily_pauses(workday.date)?,
            long_pauses: Pauses::new()?
                .set_min_duration(monitor_config.min_pause_duration)
                .get_daily_pauses(workday.date)?,
            config: productivity_config,
        })
    }

    /// Creates a productivity calculator with provided test data.
    ///
    /// This constructor is primarily intended for testing scenarios where you want
    /// to provide specific pause and break data without database dependencies.
    /// It uses default productivity configuration settings.
    ///
    /// # Arguments
    ///
    /// * `workday` - The workday record to analyze
    /// * `breaks` - Manual breaks to include in calculations
    /// * `short_pauses` - Short automatic pauses (< threshold)
    /// * `long_pauses` - Long automatic pauses (>= threshold)
    ///
    /// # Examples
    ///
    /// ```rust
    /// let productivity = Productivity::with_test_data(
    ///     &workday,
    ///     vec![],
    ///     vec![],
    ///     vec![]
    /// );
    /// let result = productivity.calculate_productivity();
    /// ```
    pub fn with_test_data(workday: &Workday, breaks: Vec<Break>, short_pauses: Vec<Pause>, long_pauses: Vec<Pause>) -> Self {
        Self {
            workday: workday.clone(),
            breaks,
            short_pauses,
            long_pauses,
            config: ProductivityConfig::default(),
        }
    }

    /// Calculates the break duration needed to reach target productivity.
    ///
    /// This function determines how many minutes of manual breaks need to be added
    /// to achieve a specific productivity threshold. This is used for generating
    /// break recommendations when productivity falls below acceptable levels.
    ///
    /// ## Calculation Logic
    ///
    /// The function works backwards from the target productivity:
    /// 1. Calculate current net work time and gross time using internal data
    /// 2. Determine required available work time for target productivity
    /// 3. Calculate needed break duration to achieve that available work time
    /// 4. Account for existing manual breaks in the calculation
    ///
    /// ## Productivity Improvement Strategy
    ///
    /// By adding manual breaks:
    /// - **Gross time**: Remains the same (workday boundaries unchanged)
    /// - **Available work time**: Decreases (manual breaks excluded)
    /// - **Net work time**: Decreases slightly (existing pauses unchanged)
    /// - **Productivity ratio**: Improves (net/available increases)
    ///
    /// ## Data Sources
    ///
    /// This method uses data already loaded in the struct:
    /// - `self.workday` for timing boundaries
    /// - `self.long_pauses` for pause calculations
    /// - `self.breaks` for existing manual breaks
    /// - `self.config.min_productivity_threshold` as the default target
    ///
    /// # Arguments
    ///
    /// * `target_productivity` - Optional desired productivity percentage (0.0-100.0).
    ///   If None, uses the configured minimum productivity threshold.
    ///
    /// # Returns
    ///
    /// Returns the number of minutes of breaks needed to reach the target,
    /// or 0 if the target is already achieved or impossible to reach.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let productivity = Productivity::new(&workday)?;
    ///
    /// // Use default threshold from config
    /// let needed_minutes = productivity.calculate_needed_break_duration(None);
    ///
    /// // Use custom threshold
    /// let needed_minutes = productivity.calculate_needed_break_duration(Some(75.0));
    ///
    /// if needed_minutes > 0 {
    ///     println!("Add {} minutes of breaks to improve productivity", needed_minutes);
    /// }
    /// ```
    pub fn calculate_needed_break_duration(&self, target_productivity: Option<f64>) -> u64 {
        let end_time = self.workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());
        let gross_duration = end_time - self.workday.start;

        // Use configured threshold if no target specified
        let target_productivity = target_productivity.unwrap_or(self.config.min_productivity_threshold);

        // Calculate current state using internal data
        let pause_duration: Duration = self.long_pauses.iter().filter_map(|p| p.duration).sum();
        let existing_break_duration: Duration = self.breaks.iter().map(|b| b.duration).sum();
        let net_work_time = gross_duration - pause_duration - existing_break_duration;

        // Validate input parameters
        if target_productivity <= 0.0 || target_productivity > 100.0 {
            return 0; // Invalid target productivity
        }

        if net_work_time.num_seconds() <= 0 {
            return 0; // No net work time available
        }

        // Calculate required available work time for target productivity
        // target_productivity = (net_work_time / available_work_time) * 100
        // available_work_time = net_work_time * 100 / target_productivity
        let required_available_time_seconds = (net_work_time.num_seconds() as f64 * 100.0 / target_productivity) as i64;
        let required_available_time = Duration::seconds(required_available_time_seconds);

        // Calculate total break duration needed
        // available_work_time = gross_duration - total_break_duration
        // total_break_duration = gross_duration - required_available_time
        let total_needed_break_duration = gross_duration - required_available_time;

        // Calculate additional break duration needed beyond existing breaks
        let additional_break_duration = total_needed_break_duration - existing_break_duration;

        // Return additional minutes needed, ensuring non-negative result
        if additional_break_duration.num_minutes() > 0 {
            additional_break_duration.num_minutes() as u64
        } else {
            0
        }
    }

    /// Check if productivity suggestions should be made based on workday progress.
    ///
    /// This function determines whether enough of the workday has passed to make
    /// meaningful productivity recommendations. It prevents premature suggestions
    /// when the workday has just started.
    ///
    /// ## Configuration Sources
    ///
    /// This method uses configuration data already loaded in the struct:
    /// - `self.config.workday_hours` for expected workday duration
    /// - `self.config.min_workday_fraction_before_suggest` for minimum elapsed fraction
    /// - `self.workday.start` for timing calculations
    ///
    /// # Returns
    ///
    /// `true` if suggestions should be made, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust
    /// let productivity = Productivity::new(&workday)?;
    /// if productivity.should_suggest_productivity_improvements() {
    ///     // Make productivity recommendations
    /// }
    /// ```
    pub fn should_suggest_productivity_improvements(&self) -> bool {
        let now = chrono::Local::now().naive_local();
        let elapsed = now - self.workday.start;
        let expected_duration = Duration::seconds((self.config.workday_hours * 3600.0) as i64);
        let min_duration = Duration::seconds((expected_duration.num_seconds() as f64 * self.config.min_workday_fraction_before_suggest) as i64);

        elapsed >= min_duration
    }

    /// Check if productivity recommendations should be shown and calculate needed break duration.
    ///
    /// This function combines productivity checking with break duration calculation to provide
    /// a complete recommendation system. It checks if suggestions should be made and calculates
    /// the break duration needed to reach the target productivity threshold.
    ///
    /// ## Self-Contained Logic
    ///
    /// This method uses all data already loaded in the struct:
    /// - `self.config` for productivity thresholds and timing rules
    /// - `self.workday` for timing calculations
    /// - `self.long_pauses` and `self.breaks` for break calculations
    /// - Internal methods for consistent calculations
    ///
    /// ## Decision Flow
    ///
    /// 1. **Timing Check**: Verify enough workday time has elapsed
    /// 2. **Productivity Check**: Calculate current productivity level
    /// 3. **Threshold Check**: Compare against minimum acceptable productivity
    /// 4. **Recommendation Calculation**: Determine needed break duration
    /// 5. **Feasibility Check**: Ensure recommendation is practical
    ///
    /// # Returns
    ///
    /// Returns `Some(needed_minutes)` if recommendations should be shown,
    /// `None` if productivity is acceptable or recommendations shouldn't be made yet.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let productivity = Productivity::new(&workday)?;
    ///
    /// if let Some(needed_minutes) = productivity.check_productivity_recommendations() {
    ///     println!("Consider adding {} minutes of breaks", needed_minutes);
    /// }
    /// ```
    pub fn check_productivity_recommendations(&self) -> Option<u64> {
        // Check if enough of the workday has passed to make suggestions
        if !self.should_suggest_productivity_improvements() {
            return None; // Too early to suggest improvements
        }

        // Calculate current productivity using internal comprehensive calculation
        let current_productivity = self.calculate_productivity();

        // Check if productivity is below the minimum threshold
        if current_productivity >= self.config.min_productivity_threshold {
            return None; // Productivity is acceptable
        }

        // Calculate needed break duration to reach minimum productivity
        let needed_minutes = self.calculate_needed_break_duration(None); // Use default threshold

        // Only show recommendations if a meaningful break can help
        if needed_minutes >= self.config.min_break_duration && needed_minutes <= self.config.max_break_duration {
            Some(needed_minutes)
        } else {
            None
        }
    }

    /// Calculates productivity percentage for the workday.
    ///
    /// This is the central productivity calculation method that properly handles different
    /// types of work interruptions to provide accurate productivity metrics. The method
    /// implements a sophisticated calculation that distinguishes between various types of
    /// time allocation.
    ///
    /// ## Calculation Logic
    ///
    /// The productivity calculation follows this formula:
    /// ```text
    /// Productivity = (Net Work Time / Available Work Time) * 100
    ///
    /// Where:
    /// - Gross Duration = End Time - Start Time
    /// - Available Work Time = Gross Duration - Manual Breaks - Long Pauses  
    /// - Net Work Time = Available Work Time - Short Pauses (adjusted for overlaps)
    /// ```
    ///
    /// ## Time Categories
    ///
    /// 1. **Manual Breaks**: User-defined break periods (excluded from work time)
    /// 2. **Long Pauses**: Automatic pauses >= min_pause_duration (recorded in DB)
    /// 3. **Short Pauses**: Automatic pauses < min_pause_duration (not recorded in DB)
    /// 4. **Active Work**: Time when user is actively working
    ///
    /// ## Overlap Handling
    ///
    /// Short pauses are adjusted to avoid double-counting time that's already
    /// accounted for in manual breaks:
    /// - If short_pause_duration <= break_duration: Set short pauses to zero
    /// - Otherwise: Subtract break duration from short pauses
    ///
    /// ## Edge Cases
    ///
    /// - Returns 0.0% if no available work time exists
    /// - Clamps result between 0.0% and 100.0% to handle calculation edge cases
    /// - Handles ongoing workdays by using current time as end time
    ///
    /// # Returns
    ///
    /// Productivity percentage as a float between 0.0 and 100.0.
    ///
    /// # Examples
    ///
    /// ```rust
    /// let productivity = productivity_instance.calculate_productivity();
    ///
    /// if productivity >= 75.0 {
    ///     println!("Good productivity: {:.1}%", productivity);
    /// } else {
    ///     println!("Consider taking a break to improve focus");
    /// }
    /// ```
    pub fn calculate_productivity(&self) -> f64 {
        let end_time = self.workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());
        let gross_duration = end_time - self.workday.start;

        // Calculate manual break time (user-defined breaks)
        let break_duration: Duration = self.breaks.iter().map(|b| b.duration).sum();

        // Calculate long pause time (automatic pauses >= min_pause_duration, recorded in DB)
        let long_pause_duration: Duration = self.long_pauses.iter().filter_map(|p| p.duration).sum();

        // Calculate short pause time (automatic pauses < min_pause_duration, not in DB)
        let mut short_pause_duration: Duration = self.short_pauses.iter().filter_map(|p| p.duration).sum();

        // Adjust short pauses to avoid double-counting with manual breaks
        short_pause_duration = if short_pause_duration <= break_duration {
            Duration::zero()
        } else {
            short_pause_duration - break_duration
        };

        // Available work time excludes manual breaks and long pauses
        let work_time = gross_duration - break_duration - long_pause_duration;

        // Net work time further excludes short pauses
        let net_work_time = work_time - short_pause_duration;

        // Calculate productivity percentage
        if work_time.num_seconds() > 0 {
            let productivity = (net_work_time.num_seconds() as f64 / work_time.num_seconds() as f64) * 100.0;
            productivity.max(0.0).min(100.0) // Clamp between 0-100%
        } else {
            0.0
        }
    }
}
