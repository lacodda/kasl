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

/// The central productivity calculation, with the data it runs on.
///
/// The split into short and long pauses drives the whole formula: long
/// pauses shrink the available time, short ones count against it.
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
    /// Loads the workday's pauses, split at `min_pause_duration` from the
    /// monitor config.
    ///
    /// ```rust,no_run
    /// # fn f() -> anyhow::Result<()> {
    /// use kasl::libs::productivity::Productivity;
    /// use kasl::db::workdays::Workdays;
    /// use chrono::Local;
    ///
    /// let mut workdays = Workdays::new()?;
    /// let workday = workdays.fetch(Local::now().date_naive())?.unwrap();
    /// let productivity = Productivity::new(&workday)?;
    /// let current_productivity = productivity.calculate_productivity();
    /// println!("Current productivity: {:.1}%", current_productivity);
    /// # Ok(())
    /// # }
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

    /// Builds a calculator from explicit data, bypassing config and database -
    /// for tests.
    ///
    /// ```rust,no_run
    /// # fn f() {
    /// use kasl::libs::productivity::Productivity;
    /// use kasl::db::workdays::Workday;
    /// use chrono::Local;
    ///
    /// let workday = Workday {
    ///     id: 1,
    ///     date: Local::now().date_naive(),
    ///     start: Local::now().naive_local(),
    ///     end: None,
    /// };
    /// let productivity = Productivity::with_test_data(
    ///     &workday,
    ///     vec![],
    ///     vec![]
    /// );
    /// let result = productivity.calculate_productivity();
    /// # let _ = result;
    /// # }
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

    /// Returns the day's productivity percentage, clamped to 0-100.
    ///
    /// `(work - short pauses) / work`, where `work` is the gross day minus
    /// long pauses. An ongoing day is measured up to now (via
    /// [`crate::libs::report::workday_end_time`]); a day with no work time
    /// yet reads 0.
    ///
    /// ```rust
    /// use kasl::libs::productivity::Productivity;
    /// use kasl::db::workdays::Workday;
    /// use chrono::Local;
    ///
    /// let workday = Workday {
    ///     id: 1,
    ///     date: Local::now().date_naive(),
    ///     start: Local::now().naive_local(),
    ///     end: None,
    /// };
    /// let productivity_instance = Productivity::with_test_data(&workday, vec![], vec![]);
    /// let productivity = productivity_instance.calculate_productivity();
    ///
    /// if productivity >= 75.0 {
    ///     println!("Good productivity: {:.1}%", productivity);
    /// } else {
    ///     println!("Consider taking a break to improve focus");
    /// }
    /// ```
    pub fn calculate_productivity(&self) -> f64 {
        let end_time = crate::libs::report::workday_end_time(&self.workday, &self.long_pauses);
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
