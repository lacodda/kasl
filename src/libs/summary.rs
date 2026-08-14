//! Monthly summary math: fold in rest days, total and average, format.
//!
//! ```rust,no_run
//! use kasl::libs::summary::{DailySummary, SummaryCalculator, SummaryFormatter};
//! use chrono::{Duration, NaiveDate};
//! use std::collections::HashSet;
//!
//! let summaries = vec![
//!     DailySummary {
//!         date: NaiveDate::from_ymd_opt(2025, 8, 11).unwrap(),
//!         duration: Duration::hours(8),
//!         productivity: 85.5,
//!     },
//! ];
//!
//! let rest_dates = HashSet::new();
//! let (processed, total, average) = summaries
//!     .add_rest_dates(rest_dates, Duration::hours(8))
//!     .calculate_totals();
//! ```

use crate::libs::formatter::format_duration;
use chrono::{Duration, NaiveDate};
use std::collections::{HashMap, HashSet};

/// One day's line in the monthly summary.
#[derive(Debug, Clone)]
pub struct DailySummary {
    /// Local calendar date.
    pub date: NaiveDate,

    /// Net productive time: presence minus pauses.
    pub duration: Duration,

    /// Productivity percentage for the day, 0-100.
    pub productivity: f64,
}

/// Builds the monthly figures by chaining transformations:
///
/// ```rust,no_run
/// use kasl::libs::summary::{DailySummary, SummaryCalculator};
/// use chrono::Duration;
/// use std::collections::HashSet;
///
/// let summaries: Vec<DailySummary> = vec![];
/// let company_holidays = HashSet::new();
/// let default_hours = Duration::hours(8);
///
/// let result = summaries
///     .add_rest_dates(company_holidays, default_hours)
///     .calculate_totals();
/// ```
pub trait SummaryCalculator {
    /// Adds an entry for each rest date the collection does not already
    /// cover, with the given default duration and zero productivity.
    ///
    /// Rest days carry default hours on purpose: the monthly report is what
    /// goes to payroll, and paid holidays count toward the monthly total
    /// while contributing no productivity.
    ///
    /// ```rust,no_run
    /// use kasl::libs::summary::{DailySummary, SummaryCalculator};
    /// use std::collections::HashSet;
    /// use chrono::{Duration, NaiveDate};
    ///
    /// let work_summaries: Vec<DailySummary> = vec![];
    /// let mut rest_dates = HashSet::new();
    /// rest_dates.insert(NaiveDate::from_ymd_opt(2025, 8, 15).unwrap()); // Company holiday
    ///
    /// let enhanced_summaries = work_summaries
    ///     .add_rest_dates(rest_dates, Duration::hours(8));
    /// ```
    fn add_rest_dates(self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> Self;

    /// Sorts by date and returns `(sorted, total, average-per-day)`;
    /// an empty collection yields zero for both figures.
    ///
    /// ```rust,no_run
    /// use kasl::libs::summary::{DailySummary, SummaryCalculator};
    /// use kasl::libs::formatter::format_duration;
    /// use std::collections::HashSet;
    /// use chrono::Duration;
    ///
    /// let summaries: Vec<DailySummary> = vec![];
    /// let rest_dates = HashSet::new();
    ///
    /// let (processed_summaries, total_hours, average_daily) = summaries
    ///     .add_rest_dates(rest_dates, Duration::hours(8))
    ///     .calculate_totals();
    ///
    /// println!("Total monthly hours: {}", format_duration(&total_hours));
    /// println!("Average daily hours: {}", format_duration(&average_daily));
    /// # let _ = processed_summaries;
    /// ```
    fn calculate_totals(self) -> (Self, Duration, Duration)
    where
        Self: Sized;
}

impl SummaryCalculator for Vec<DailySummary> {
    fn add_rest_dates(mut self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> Self {
        for rest_date in rest_dates {
            let date_exists = self.iter().any(|summary| summary.date == rest_date);

            if !date_exists {
                self.push(DailySummary {
                    date: rest_date,
                    duration,
                    productivity: 0.0, // Rest days have zero productivity
                });
            }
        }

        self
    }

    fn calculate_totals(mut self) -> (Self, Duration, Duration) {
        self.sort_by_key(|summary| summary.date);

        let total_duration = self.iter().fold(Duration::zero(), |accumulator, summary| accumulator + summary.duration);

        let day_count = self.len() as i64;
        let average_duration = if day_count > 0 {
            Duration::seconds(total_duration.num_seconds() / day_count)
        } else {
            Duration::zero()
        };

        (self, total_duration, average_duration)
    }
}

/// Renders calculated summaries for display.
pub trait SummaryFormatter {
    /// Returns `(per-date map of (duration, productivity), total, average)`,
    /// durations as `HH:MM` and productivity as `XX.X%`.
    ///
    /// ```rust,no_run
    /// use kasl::libs::summary::{DailySummary, SummaryFormatter};
    /// use chrono::Duration;
    ///
    /// let calculated_summaries: (Vec<DailySummary>, Duration, Duration) =
    ///     (vec![], Duration::zero(), Duration::zero());
    ///
    /// let (daily_map, total_str, avg_str) = calculated_summaries.format_summary();
    ///
    /// for (date, (duration, productivity)) in daily_map {
    ///     println!("{}: {} hours ({})", date, duration, productivity);
    /// }
    ///
    /// println!("Total: {}, Average: {}", total_str, avg_str);
    /// ```
    fn format_summary(&self) -> (HashMap<NaiveDate, (String, String)>, String, String);
}

impl SummaryFormatter for (Vec<DailySummary>, Duration, Duration) {
    fn format_summary(&self) -> (HashMap<NaiveDate, (String, String)>, String, String) {
        let (daily_summaries, total_duration, average_duration) = self;

        let daily_durations = daily_summaries
            .iter()
            .map(|summary| {
                let formatted_duration = format_duration(&summary.duration);
                let formatted_productivity = format!("{:.1}%", summary.productivity);
                (summary.date, (formatted_duration, formatted_productivity))
            })
            .collect();

        let total_duration_str = format_duration(total_duration);
        let average_duration_str = format_duration(average_duration);

        (daily_durations, total_duration_str, average_duration_str)
    }
}
