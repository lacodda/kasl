use crate::libs::event::FormatEvent;
use chrono::{Duration, NaiveDate};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct DailySummary {
    pub date: NaiveDate,
    pub duration: Duration,
}

pub trait SummaryCalculator {
    fn add_rest_dates(self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> Self;
    // The return type is corrected from (Vec<Self>, ...) to (Self, ...)
    fn calculate_totals(self) -> (Self, Duration, Duration)
    where
        Self: Sized;
}

impl SummaryCalculator for Vec<DailySummary> {
    fn add_rest_dates(mut self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> Self {
        for rest_date in rest_dates {
            if !self.iter().any(|ds| ds.date == rest_date) {
                self.push(DailySummary { date: rest_date, duration });
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

pub trait SummaryFormatter {
    fn format_summary(&self) -> (HashMap<NaiveDate, String>, String, String);
}

impl SummaryFormatter for (Vec<DailySummary>, Duration, Duration) {
    fn format_summary(&self) -> (HashMap<NaiveDate, String>, String, String) {
        let daily_durations = self.0.iter().map(|ds| (ds.date, FormatEvent::format_duration(Some(ds.duration)))).collect();

        let total_duration_str = FormatEvent::format_duration(Some(self.1));
        let average_duration_str = FormatEvent::format_duration(Some(self.2));

        (daily_durations, total_duration_str, average_duration_str)
    }
}
