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
//!         rest_day: false,
//!     },
//! ];
//!
//! let rest_dates = HashSet::new();
//! let (processed, total, average) = summaries
//!     .add_rest_dates(rest_dates, Duration::hours(8))
//!     .calculate_totals();
//! ```

use crate::api::si::Si;
use crate::db::workdays::Workdays;
use crate::libs::config::Config;
use crate::libs::formatter::format_duration;
use crate::libs::json;
use crate::libs::messages::Message;
use crate::libs::report::DayWork;
use crate::msg_error;
use anyhow::Result;
use chrono::{Datelike, Days, Duration, Months, NaiveDate, Weekday};
use std::collections::{HashMap, HashSet};

/// The first and the last day of the month `date` falls in.
pub fn month_bounds(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let first = date - Days::new(u64::from(date.day0()));
    // Only the last month chrono can represent has no next one to step back from.
    let last = first
        .checked_add_months(Months::new(1))
        .and_then(|next| next.pred_opt())
        .unwrap_or(NaiveDate::MAX);
    (first, last)
}

/// The Monday and the Sunday of the ISO week `date` falls in.
pub fn week_bounds(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let week = date.week(Weekday::Mon);
    // Only the first and the last week chrono can represent run past its range.
    (
        week.checked_first_day().unwrap_or(NaiveDate::MIN),
        week.checked_last_day().unwrap_or(NaiveDate::MAX),
    )
}

/// A stretch of days as the summaries read it: a month for `kasl sum`, a week
/// for the MCP `week` tool.
///
/// Read once for every way of printing it, like [`crate::libs::report::DayReport`].
/// Each recorded day is worked out by [`DayWork`] - the daily report's own
/// arithmetic - so a day reads the same in a report, a week and a month.
pub struct Summary {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Recorded days and company rest days, in date order.
    pub days: Vec<DailySummary>,
    pub total: Duration,
    pub average: Duration,
    /// The average over recorded days; `None` while there are none.
    pub productivity: Option<f64>,
}

impl Summary {
    /// The month `date` falls in.
    pub async fn month(date: NaiveDate) -> Result<Summary> {
        let (from, to) = month_bounds(date);
        Self::read(from, to).await
    }

    /// The week, Monday to Sunday, `date` falls in.
    pub async fn week(date: NaiveDate) -> Result<Summary> {
        let (from, to) = week_bounds(date);
        Self::read(from, to).await
    }

    /// Reads the days from `from` to `to`, both included, with the company's
    /// rest days among them when the reporting API is configured.
    pub async fn read(from: NaiveDate, to: NaiveDate) -> Result<Summary> {
        let config = Config::read()?;
        let monitor = config.monitor.clone().unwrap_or_default();
        let rest_dates = rest_dates(&config, from, to).await;

        let workdays = Workdays::new()?.fetch_range(from, to)?;
        let mut daily_summaries = Vec::with_capacity(workdays.len());
        let mut total_productivity = 0.0;
        for workday in &workdays {
            let day = DayWork::of(workday, &monitor)?;
            total_productivity += day.productivity;
            daily_summaries.push(DailySummary {
                date: workday.date,
                duration: day.work,
                productivity: day.productivity,
                rest_day: false,
            });
        }

        let (days, total, average) = daily_summaries
            .add_rest_dates(rest_dates, Duration::hours(8)) // Default 8 hours for rest days
            .calculate_totals();
        let recorded = workdays.len() as f64;
        let productivity = (total_productivity > 0.0 && recorded > 0.0).then(|| total_productivity / recorded);

        Ok(Summary {
            from,
            to,
            days,
            total,
            average,
            productivity,
        })
    }

    /// The days as both documents list them.
    fn json_days(&self) -> Vec<json::SummaryDay> {
        self.days
            .iter()
            .map(|day| json::SummaryDay {
                date: day.date,
                work_seconds: json::seconds(&day.duration),
                productivity: json::percent(day.productivity),
                rest_day: day.rest_day,
            })
            .collect()
    }

    /// The month as `kasl sum --json` prints it.
    pub fn to_month_json(&self) -> json::Month {
        json::Month {
            month: self.from.format("%Y-%m").to_string(),
            days: self.json_days(),
            total_seconds: json::seconds(&self.total),
            average_seconds: json::seconds(&self.average),
            productivity: self.productivity.map(json::percent),
        }
    }

    /// The week as the MCP `week` tool returns it.
    pub fn to_week_json(&self) -> json::Week {
        let week = self.from.iso_week();
        json::Week {
            week: format!("{}-W{:02}", week.year(), week.week()),
            from: self.from,
            to: self.to,
            days: self.json_days(),
            total_seconds: json::seconds(&self.total),
            average_seconds: json::seconds(&self.average),
            productivity: self.productivity.map(json::percent),
        }
    }
}

/// The company's rest days from `from` to `to`, asked of the reporting API
/// year by year; none when it is not configured.
///
/// A failed request is said on stderr and leaves the summary to local data:
/// the hours recorded on this machine are worth showing without the calendar.
async fn rest_dates(config: &Config, from: NaiveDate, to: NaiveDate) -> HashSet<NaiveDate> {
    let mut dates = HashSet::new();
    let Some(si_config) = &config.si else {
        return dates;
    };
    let mut si = Si::new(si_config);
    for year in from.year()..=to.year() {
        let Some(in_year) = NaiveDate::from_ymd_opt(year, 1, 1) else {
            continue;
        };
        match si.rest_dates(in_year).await {
            Ok(found) => dates.extend(found.into_iter().filter(|date| (from..=to).contains(date))),
            Err(e) => msg_error!(Message::ErrorRequestingRestDates(e.to_string())),
        }
    }
    dates
}

/// One day's line in the monthly summary.
#[derive(Debug, Clone)]
pub struct DailySummary {
    /// Local calendar date.
    pub date: NaiveDate,

    /// Net productive time: presence minus pauses.
    pub duration: Duration,

    /// Productivity percentage for the day, 0-100.
    pub productivity: f64,

    /// A company rest day counted as a full day, rather than a recorded one.
    pub rest_day: bool,
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
                    rest_day: true,
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
