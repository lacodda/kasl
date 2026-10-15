//! Monthly working hours summary command.
//!
//! Generates comprehensive monthly reports showing daily work hours, productivity metrics, and calendar integration with company rest days.
//!
//! ## Usage
//!
//! ```bash
//! # Generate current month summary
//! kasl sum
//!
//! # Submit monthly report to external API
//! kasl sum --send
//! ```

use crate::{
    api::si::Si,
    db::{pauses::Pauses, workdays::Workdays},
    libs::{
        config::Config,
        json,
        messages::Message,
        output::{self, Format},
        summary::{DailySummary, SummaryCalculator, SummaryFormatter},
        view::View,
    },
    msg_error, msg_print,
};
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate};
use clap::Args;
use std::collections::HashSet;

/// Command-line arguments for the monthly summary command.
///
/// Currently supports basic summary generation with optional report submission.
/// Future versions may add date range selection and detailed filtering options.
#[derive(Debug, Args)]
pub struct SumArgs {
    /// Submit the monthly summary report
    ///
    /// When specified, the generated summary will be submitted to the configured
    /// reporting API in addition to being displayed locally. This is useful for
    /// organizational reporting requirements.
    #[arg(long, help = "Send report", conflicts_with = "json")]
    send: bool,

    #[command(flatten)]
    format: Format,
}

/// The month as `kasl sum` shows it, read once for the table and for `--json`.
pub struct MonthSummary {
    /// Any day of the month.
    pub month: NaiveDate,
    /// Recorded days and company rest days, in date order.
    pub days: Vec<DailySummary>,
    pub total: Duration,
    pub average: Duration,
    /// The average over recorded days; `None` while there are none.
    pub productivity: Option<f64>,
}

impl MonthSummary {
    /// Reads the month `today` falls in, with the company's rest days when the reporting API is configured.
    pub async fn read(today: NaiveDate) -> Result<MonthSummary> {
        let config = Config::read()?;
        let monitor_config = config.monitor.clone().unwrap_or_default();

        // Company rest dates from the reporting API, if configured
        let mut rest_dates: HashSet<NaiveDate> = HashSet::new();
        if let Some(si_config) = config.si {
            match Si::new(&si_config).rest_dates(today).await {
                Ok(dates) => {
                    // Filter rest dates to only include current month
                    rest_dates = dates.into_iter().filter(|d| d.month() == today.month()).collect();
                }
                Err(e) => {
                    // Log error but continue with local data only
                    msg_error!(Message::ErrorRequestingRestDates(e.to_string()));
                }
            }
        }

        let workdays = Workdays::new()?.fetch_month(today)?;
        let workdays_count = workdays.len() as f64;
        let mut daily_summaries = Vec::new();
        let mut total_productivity = 0.0;

        for workday in workdays {
            // Now only while the day is still today; an unclosed past day ends at
            // its last observed activity - see report::workday_end_time.
            let workday_pauses = Pauses::new()?.get_workday_pauses(&workday)?;
            let end_time = crate::libs::report::workday_end_time(&workday, &workday_pauses);
            let gross_duration = end_time.signed_duration_since(workday.start);

            // Long breaks only: the gross day minus them is the time shown
            let long_breaks_duration = Pauses::new()?
                .set_min_duration(monitor_config.min_pause_duration)
                .get_workday_pauses(&workday)?
                .iter()
                .filter_map(|b| b.duration)
                .fold(Duration::zero(), |acc, d| acc + d);

            // The same calculation the daily report uses
            let productivity = crate::libs::productivity::Productivity::new(&workday)
                .map(|p| p.calculate_productivity())
                .unwrap_or(0.0);
            total_productivity += productivity;

            daily_summaries.push(DailySummary {
                date: workday.date,
                duration: gross_duration - long_breaks_duration,
                productivity,
                rest_day: false,
            });
        }

        let (days, total, average) = daily_summaries
            .add_rest_dates(rest_dates, Duration::hours(8)) // Default 8 hours for rest days
            .calculate_totals();
        let productivity = (total_productivity > 0.0 && workdays_count > 0.0).then(|| total_productivity / workdays_count);

        Ok(MonthSummary {
            month: today,
            days,
            total,
            average,
            productivity,
        })
    }

    /// The month as `--json` prints it.
    pub fn to_json(&self) -> json::Month {
        json::Month {
            month: self.month.format("%Y-%m").to_string(),
            days: self
                .days
                .iter()
                .map(|day| json::MonthDay {
                    date: day.date,
                    work_seconds: json::seconds(&day.duration),
                    productivity: json::percent(day.productivity),
                    rest_day: day.rest_day,
                })
                .collect(),
            total_seconds: json::seconds(&self.total),
            average_seconds: json::seconds(&self.average),
            productivity: self.productivity.map(json::percent),
        }
    }
}

/// Generates and displays a comprehensive monthly working hours summary.
///
/// Creates a detailed analysis of work patterns for the current month, including
/// productivity calculations, rest day integration, and daily breakdowns.
///
pub async fn cmd(args: SumArgs) -> Result<()> {
    let now = Local::now();
    let month = MonthSummary::read(now.date_naive()).await?;
    if args.format.json {
        return output::print_json(&month.to_json());
    }

    msg_print!(Message::WorkingHoursForMonth(now.format("%B, %Y").to_string()), true);
    let event_summary = (month.days, month.total, month.average).format_summary();
    View::sum(&event_summary)?;
    if let Some(average_productivity) = month.productivity {
        msg_print!(Message::MonthlyProductivity(average_productivity), true);
    }

    Ok(())
}
