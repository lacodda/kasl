use crate::{
    api::si::Si,
    db::{pauses::Pauses, workdays::Workdays},
    libs::{
        config::Config,
        summary::{DailySummary, SummaryCalculator, SummaryFormatter},
        view::View,
    },
};
use chrono::{Datelike, Duration, Local, NaiveDate};
use clap::Args;
use std::{collections::HashSet, error::Error};

#[derive(Debug, Args)]
pub struct SumArgs {
    #[arg(long, help = "Send report")]
    send: bool,
}

pub async fn cmd(_sum_args: SumArgs) -> Result<(), Box<dyn Error>> {
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
    let mut daily_summaries = Vec::new();

    // 3. Calculate net time for each workday
    for workday in workdays {
        let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
        let gross_duration = end_time.signed_duration_since(workday.start);

        let pauses = Pauses::new()?
            .fetch(workday.date, monitor_config.min_pause_duration)?
            .iter()
            .filter_map(|b| b.duration)
            .fold(Duration::zero(), |acc, d| acc + d);

        let net_duration = gross_duration - pauses;
        daily_summaries.push(DailySummary {
            date: workday.date,
            duration: net_duration,
        });
    }

    // 4. Combine with rest dates, calculate totals, and format for view
    let event_summary = daily_summaries
        .add_rest_dates(rest_dates, Duration::hours(8))
        .calculate_totals()
        .format_summary();

    View::sum(&event_summary)?;

    Ok(())
}
