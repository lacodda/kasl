//! Provides structs and traits for calculating and formatting monthly summaries.

use crate::libs::formatter::format_duration;
use chrono::{Duration, NaiveDate};
use std::collections::{HashMap, HashSet};

/// Represents a summary of work duration for a single day.
#[derive(Debug, Clone)]
pub struct DailySummary {
    /// The specific date of the summary.
    pub date: NaiveDate,
    /// The total net work duration for that day.
    pub duration: Duration,
    /// Work productivity for that day.
    pub productivity: f64,
}

/// A trait for calculators that process collections of `DailySummary`.
pub trait SummaryCalculator {
    /// Adds rest days to the summary collection.
    ///
    /// If a rest date is not already present in the summaries, it's added
    /// with a default work duration (e.g., 8 hours).
    fn add_rest_dates(self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> Self;

    /// Calculates the total and average work durations for the summary collection.
    fn calculate_totals(self) -> (Self, Duration, Duration)
    where
        Self: Sized;
}

impl SummaryCalculator for Vec<DailySummary> {
    fn add_rest_dates(mut self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> Self {
        for rest_date in rest_dates {
            if !self.iter().any(|ds| ds.date == rest_date) {
                self.push(DailySummary {
                    date: rest_date,
                    duration,
                    productivity: 0.0,
                });
            }
        }
        self
    }

    fn calculate_totals(mut self) -> (Self, Duration, Duration) {
        self.sort_by_key(|ds| ds.date);
        let total_duration = self.iter().fold(Duration::zero(), |acc, ds| acc + ds.duration);

        let count = self.len() as i64;
        let average_duration = if count > 0 {
            Duration::seconds(total_duration.num_seconds() / count)
        } else {
            Duration::zero()
        };
        (self, total_duration, average_duration)
    }
}

/// A trait for formatting calculated summaries into displayable strings.
pub trait SummaryFormatter {
    /// Formats the summary data into a map of daily durations and total/average strings.
    fn format_summary(&self) -> (HashMap<NaiveDate, (String, String)>, String, String);
}

impl SummaryFormatter for (Vec<DailySummary>, Duration, Duration) {
    fn format_summary(&self) -> (HashMap<NaiveDate, (String, String)>, String, String) {
        let daily_durations = self
            .0
            .iter()
            .map(|ds| (ds.date, (format_duration(&ds.duration), format!("{:.1}%", &ds.productivity))))
            .collect();
        let total_duration_str = format_duration(&self.1);
        let average_duration_str = format_duration(&self.2);

        (daily_durations, total_duration_str, average_duration_str)
    }
}
