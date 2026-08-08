//! Productivity calculation utilities for work time analysis.
//!
//! This module provides centralized productivity calculations that distinguish
//! brief interruptions from full absences to give accurate productivity metrics.
//!
//! ## Key Concepts
//!
//! - **Short Pauses**: Brief interruptions below `min_pause_duration`
//! - **Long Pauses**: Full absences at or above `min_pause_duration`, plus any
//!   manual pauses the user recorded (protected records bypass the threshold)
//!
//! ## Productivity Formula
//!
//! ```text
//! Productivity = (Net Work Time / Available Work Time) * 100
//!
//! Where:
//! - Available Work Time = Total Time - Long Pauses
//! - Net Work Time = Available Work Time - Short Pauses
//! ```

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
    /// Short automatic pauses (< min_pause_duration, not in database)
    pub short_pauses: Vec<Pause>,
    /// Long pauses (>= min_pause_duration, plus any manual protected pauses)
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
            short_pauses: Pauses::new()?.set_max_duration(monitor_config.min_pause_duration).get_workday_pauses(workday)?,
            long_pauses: Pauses::new()?.set_min_duration(monitor_config.min_pause_duration).get_workday_pauses(workday)?,
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
    pub fn with_test_data(workday: &Workday, short_pauses: Vec<Pause>, long_pauses: Vec<Pause>) -> Self {
        Self {
            workday: workday.clone(),
            short_pauses,
            long_pauses,
            config: ProductivityConfig::default(),
        }
    }

    /// Reports whether productivity has fallen below the configured threshold.
    ///
    /// The check is suppressed early in the day: a short elapsed period makes the
    /// ratio swing wildly on a single pause, so warning then would be noise. Once
    /// `min_workday_fraction_before_suggest` of the expected workday has passed,
    /// the figure is stable enough to act on.
    ///
    /// # Returns
    ///
    /// `true` when enough of the day has elapsed and productivity is under
    /// `min_productivity_threshold`.
    pub fn is_below_threshold(&self) -> bool {
        let now = chrono::Local::now().naive_local();
        let elapsed = now - self.workday.start;
        let expected_duration = Duration::seconds((self.config.workday_hours * 3600.0) as i64);
        let min_elapsed = Duration::seconds((expected_duration.num_seconds() as f64 * self.config.min_workday_fraction_before_suggest) as i64);

        if elapsed < min_elapsed {
            return false;
        }

        self.calculate_productivity() < self.config.min_productivity_threshold
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

        // Long pauses: detected absences above the threshold, plus any manual
        // pauses the user recorded (protected records bypass the threshold).
        let long_pause_duration: Duration = self.long_pauses.iter().filter_map(|p| p.duration).sum();

        // Short pauses: brief interruptions below the threshold.
        let short_pause_duration: Duration = self.short_pauses.iter().filter_map(|p| p.duration).sum();

        // Available work time excludes time the user was away entirely
        let work_time = gross_duration - long_pause_duration;

        // Net work time further excludes short pauses
        let net_work_time = work_time - short_pause_duration;

        // Calculate productivity percentage
        if work_time.num_seconds() > 0 {
            let productivity = (net_work_time.num_seconds() as f64 / work_time.num_seconds() as f64) * 100.0;
            productivity.clamp(0.0, 100.0)
        } else {
            0.0
        }
    }
}
