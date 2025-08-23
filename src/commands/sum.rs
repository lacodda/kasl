//! Monthly working hours summary command.
//!
//! This command generates comprehensive monthly reports showing daily work hours,
//! productivity metrics using the centralized Productivity module, and calendar 
//! integration with company rest days. It provides both detailed daily breakdowns 
//! and aggregate statistics for the current month.

use crate::{
    api::si::Si,
    db::{pauses::Pauses, workdays::Workdays},
    libs::{
        config::Config,
        messages::Message,
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
    #[arg(long, help = "Send report")]
    send: bool,
}

/// Generates and displays a comprehensive monthly working hours summary.
///
/// Creates a detailed analysis of work patterns for the current month, including
/// productivity calculations, rest day integration, and daily breakdowns.
///
/// # Arguments
///
/// * `_sum_args` - Command arguments (currently unused but reserved for future features)
///
/// # Returns
///
/// Returns `Ok(())` on successful summary generation and display, or an error
/// if data retrieval or calculation fails.
/// - API connectivity issues (for rest dates)
/// - Configuration errors
/// - Invalid date calculations
/// - Missing workday data
pub async fn cmd(_sum_args: SumArgs) -> Result<()> {
    let now = Local::now();
    let config = Config::read()?;
    let monitor_config = config.monitor.clone().unwrap_or_default();

    // Display header with current month and year
    msg_print!(Message::WorkingHoursForMonth(now.format("%B, %Y").to_string()), true);

    // Step 1: Fetch company rest dates from external API if configured
    let mut rest_dates: HashSet<NaiveDate> = HashSet::new();
    if let Some(si_config) = config.si {
        match Si::new(&si_config).rest_dates(now.date_naive()).await {
            Ok(dates) => {
                // Filter rest dates to only include current month
                rest_dates = dates.into_iter().filter(|d| d.month() == now.month()).collect();
            }
            Err(e) => {
                // Log error but continue with local data only
                msg_error!(Message::ErrorRequestingRestDates(e.to_string()));
            }
        }
    }

    // Step 2: Fetch all workdays for the current month from local database
    let workdays = Workdays::new()?.fetch_month(now.date_naive())?;
    let workdays_count = workdays.len() as f64;
    let mut daily_summaries = Vec::new();
    let mut total_productivity = 0.0;

    // Step 3: Process each workday to calculate durations and productivity
    for workday in workdays {
        let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
        let gross_duration = end_time.signed_duration_since(workday.start);

        // Note: All pauses data now handled by Productivity module

        // Fetch filtered long breaks for display purposes
        let long_breaks_duration = Pauses::new()?
            .set_min_duration(monitor_config.min_pause_duration)
            .get_daily_pauses(workday.date)?
            .iter()
            .filter_map(|b| b.duration)
            .fold(Duration::zero(), |acc, d| acc + d);

        // Calculate display duration (gross time minus long breaks only)
        let gross_work_time_minus_long_breaks = gross_duration - long_breaks_duration;

        // Note: Net working duration calculation now handled by Productivity module

        // Calculate productivity using centralized module for consistency
        // This uses the same comprehensive calculation logic used throughout the app
        let productivity = crate::libs::productivity::Productivity::new(&workday)
            .map(|p| p.calculate_productivity())
            .unwrap_or(0.0);

        // Accumulate productivity for monthly average calculation
        total_productivity += productivity;

        // Create daily summary entry
        daily_summaries.push(DailySummary {
            date: workday.date,
            duration: gross_work_time_minus_long_breaks, // Display duration
            productivity,
        });
    }

    // Step 4: Integrate rest dates and calculate summary statistics
    let event_summary = daily_summaries
        .add_rest_dates(rest_dates, Duration::hours(8)) // Default 8 hours for rest days
        .calculate_totals()
        .format_summary();

    // Step 5: Display the formatted summary table
    View::sum(&event_summary)?;

    // Step 6: Display monthly productivity average
    if total_productivity > 0.0 && workdays_count > 0.0 {
        let average_productivity = total_productivity / workdays_count;
        msg_print!(Message::MonthlyProductivity(average_productivity), true);
    }

    Ok(())
}
