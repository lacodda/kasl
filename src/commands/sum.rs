use crate::{
    api::si::Si,
    db::{pauses::Pauses, workdays::Workdays},
    libs::{
        config::Config,
        summary::{DailySummary, SummaryCalculator, SummaryFormatter},
        view::View,
    },
};
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate};
use clap::Args;
use std::collections::HashSet;

#[derive(Debug, Args)]
pub struct SumArgs {
    #[arg(long, help = "Send report")]
    send: bool,
}

pub async fn cmd(_sum_args: SumArgs) -> Result<()> {
    let now = Local::now();
    let config = Config::read()?;
    let monitor_config = config.monitor.clone().unwrap_or_default();

    println!("\nWorking hours for {}", now.format("%B, %Y"));

    // 1. Fetch rest dates from API
    let mut rest_dates: HashSet<NaiveDate> = HashSet::new();
    if let Some(si_config) = config.si {
        match Si::new(&si_config).rest_dates(now.date_naive()).await {
            Ok(dates) => {
                // Filter for dates in the current month
                rest_dates = dates.into_iter().filter(|d| d.month() == now.month()).collect();
            }
            Err(e) => eprintln!("Error requesting rest dates: {}", e),
        }
    }

    // 2. Fetch all workdays for the current month
    let workdays = Workdays::new()?.fetch_month(now.date_naive())?;
    let workdays_count = workdays.len() as f64;
    let mut daily_summaries = Vec::new();
    let mut total_productivity = 0.0;

    // 3. Calculate net time for each workday
    for workday in workdays {
        let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
        let gross_duration = end_time.signed_duration_since(workday.start);

        // Use ALL pauses for productivity calculation (min_duration = 0)
        let all_pauses_duration = Pauses::new()?
            .fetch(workday.date, 0)?
            .iter()
            .filter_map(|b| b.duration)
            .fold(Duration::zero(), |acc, d| acc + d);

        // But for display purposes, we'll use the filtered pauses
        let long_breaks_duration = Pauses::new()?
            .fetch(workday.date, monitor_config.min_pause_duration)?
            .iter()
            .filter_map(|b| b.duration)
            .fold(Duration::zero(), |acc, d| acc + d);

        // Net duration for display uses filtered long breaks
        let gross_work_time_minus_long_breaks = gross_duration - long_breaks_duration;
        // Net duration for productivity uses ALL pauses
        let net_working_duration = gross_duration - all_pauses_duration;

        // Calculate productivity as (net working duration / gross work time minus long breaks) * 100.
        // This gives the percentage of time truly spent productively out of the time "on duty"
        // (excluding only major breaks).
        let productivity = (net_working_duration.num_seconds() as f64 / gross_work_time_minus_long_breaks.num_seconds() as f64) * 100.0;

        // Accumulate productivity
        total_productivity = total_productivity + productivity;

        daily_summaries.push(DailySummary {
            date: workday.date,
            duration: gross_work_time_minus_long_breaks, // Use filtered duration for display
            productivity,
        });
    }

    // 4. Combine with rest dates, calculate totals, and format for view
    let event_summary = daily_summaries
        .add_rest_dates(rest_dates, Duration::hours(8))
        .calculate_totals()
        .format_summary();

    View::sum(&event_summary)?;

    // 5. Display monthly productivity (calculated with ALL pauses)
    if total_productivity > 0.0 && workdays_count > 0.0 {
        println!("\nMonthly work productivity: {:.1}%", total_productivity / workdays_count);
    }

    Ok(())
}
