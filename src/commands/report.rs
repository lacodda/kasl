//! Contains the logic for the `report` command.
//!
//! This command handles the generation, display, and submission of daily
//! and monthly work reports.

use crate::{
    api::si::Si,
    db::{
        pauses::Pauses,
        tasks::Tasks,
        workdays::{Workday, Workdays},
    },
    libs::{
        config::Config,
        formatter::format_duration,
        pause::Pause,
        report,
        task::{FormatTasks, Task, TaskFilter},
        view::View,
    },
};
use chrono::{DateTime, Duration, Local};
use clap::Args;
use serde_json::json;
use std::error::Error;

/// Command-line arguments for the `report` command.
#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Submits the daily report to the configured API.
    #[arg(long, help = "Submit daily report")]
    send: bool,
    /// Generates a report for the previous day instead of the current day.
    #[arg(long, short, help = "Generate report for the last day")]
    last: bool,
    /// Submits a monthly summary report to the API.
    #[arg(long, help = "Submit monthly report")]
    month: bool,
}

/// Main entry point for the `report` command.
///
/// This function acts as a dispatcher based on the provided arguments,
/// determining the target date and delegating to the appropriate handler for
/// daily, monthly, display, or send actions.
pub async fn cmd(args: ReportArgs) -> Result<(), Box<dyn Error>> {
    let date = determine_report_date(args.last);
    if args.month {
        handle_monthly_report(date).await
    } else {
        handle_daily_report(args.send, date).await
    }
}

/// Determines the target date for the report (either today or yesterday).
fn determine_report_date(is_last_day: bool) -> DateTime<Local> {
    if is_last_day {
        Local::now() - Duration::days(1)
    } else {
        Local::now()
    }
}

/// Handles the logic for daily reports, dispatching to either the submission or display handler.
async fn handle_daily_report(should_send: bool, date: DateTime<Local>) -> Result<(), Box<dyn Error>> {
    if should_send {
        send_daily_report(date).await
    } else {
        display_daily_report(date).await
    }
}

/// Handles the submission of the monthly report.
///
/// It initializes the API service and sends the report. In case of a network
/// error, it prints a message to `stderr` instead of crashing.
async fn handle_monthly_report(date: DateTime<Local>) -> Result<(), Box<dyn Error>> {
    let mut si = get_si_service()?;
    let naive_date = date.date_naive();

    match si.send_monthly(&naive_date).await {
        Ok(status) => {
            if status.is_success() {
                println!(
                    "Your monthly report dated {} has been successfully submitted\nWait for a message to your email address",
                    date.format("%B %-d, %Y")
                );
            } else {
                println!("Failed to send monthly report. Status: {}", status);
            }
        }
        Err(e) => eprintln!("[kasl] Error sending monthly report: {}", e),
    }

    Ok(())
}

/// Fetches all necessary data and displays a formatted daily report in the terminal.
async fn display_daily_report(date: DateTime<Local>) -> Result<(), Box<dyn Error>> {
    let naive_date = date.date_naive();
    let workday = match Workdays::new()?.fetch(naive_date)? {
        Some(wd) => wd,
        None => {
            println!("\nNo workday record found for {}", date.format("%B %-d, %Y"));
            return Ok(());
        }
    };

    let tasks = Tasks::new()?.fetch(TaskFilter::Date(naive_date))?;
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();

    // Fetch long breaks for display
    let long_breaks = Pauses::new()?.fetch(naive_date, monitor_config.min_pause_duration)?;
    // Fetch ALL pauses for productivity calculation
    let all_pauses = Pauses::new()?.fetch(naive_date, 0)?;

    View::report(&workday, &long_breaks, &all_pauses, &tasks)?;
    Ok(())
}

/// Handles the entire process of sending a daily report to the API.
///
/// This includes finalizing the workday, fetching data, building the JSON payload,
/// and submitting it. It also triggers a monthly report if it's the last working
/// day of the month.
async fn send_daily_report(date: DateTime<Local>) -> Result<(), Box<dyn Error>> {
    let naive_date = date.date_naive();
    let mut workdays_db = Workdays::new()?;

    // Finalize the workday before sending the report.
    workdays_db.insert_end(naive_date)?;
    // Load the data needed for the report.
    let workday = workdays_db
        .fetch(naive_date)?
        .ok_or_else(|| format!("Could not find workday for {} after finalizing.", naive_date))?;

    let mut tasks = Tasks::new()?.fetch(TaskFilter::Date(naive_date))?;
    if tasks.is_empty() {
        println!("Tasks not found for {}, report not sent.", date.format("%B %-d, %Y"));
        return Ok(());
    }

    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();
    let pauses = Pauses::new()?.fetch(naive_date, monitor_config.min_pause_duration)?;

    let report_json = build_report_payload(&workday, &mut tasks, &pauses);
    let events_json = serde_json::to_string(&report_json)?;
    let mut si = get_si_service()?;

    match si.send(&events_json, &naive_date).await {
        Ok(status) => {
            if status.is_success() {
                println!(
                    "Your report dated {} has been successfully submitted\nWait for a message to your email address",
                    date.format("%B %-d, %Y")
                );
                // If it's the last working day of the month, also send the monthly report.
                if si.is_last_working_day_of_month(&naive_date)? {
                    println!("It's the last working day of the month. Submitting the monthly report as well...");
                    handle_monthly_report(date).await?;
                }
            } else {
                println!("Failed to send report. Status: {}", status);
            }
        }
        Err(e) => eprintln!("[kasl] Error sending events: {}", e),
    }

    Ok(())
}

/// Builds the JSON payload for the API submission based on work intervals.
fn build_report_payload(workday: &Workday, tasks: &mut Vec<Task>, pauses: &[Pause]) -> serde_json::Value {
    // General logic to calculate intervals
    let intervals = report::calculate_work_intervals(workday, pauses);

    let num_tasks = tasks.len();
    let num_intervals = intervals.len();

    if num_intervals == 0 {
        return json!([]);
    }

    let mut report_items = Vec::new();

    // Logic of task distribution by intervals
    if num_tasks >= num_intervals {
        // If tasks are greater than or equal to Intervals
        let mut task_iter = tasks.iter();
        let base_tasks_per_interval = num_tasks / num_intervals;
        let mut extra_tasks = num_tasks % num_intervals;

        for (i, interval) in intervals.iter().enumerate() {
            let count = base_tasks_per_interval + if extra_tasks > 0 { 1 } else { 0 };
            if extra_tasks > 0 {
                extra_tasks -= 1;
            }

            let mut assigned_tasks: Vec<Task> = task_iter.by_ref().take(count).cloned().collect();

            report_items.push(json!({
                "from": interval.start.format("%H:%M").to_string(),
                "index": i + 1,
                "result": "",
                "task": assigned_tasks.format(),
                "time": "",
                "to": interval.end.format("%H:%M").to_string(),
                "total_ts": format_duration(&interval.duration)
            }));
        }
    } else {
        // If there are more intervals than tasks
        let mut interval_iter = intervals.iter();
        let base_intervals_per_task = num_intervals / num_tasks;
        let mut extra_intervals = num_intervals % num_tasks;

        for task in tasks.iter() {
            let count = base_intervals_per_task + if extra_intervals > 0 { 1 } else { 0 };
            if extra_intervals > 0 {
                extra_intervals -= 1;
            }

            for _ in 0..count {
                if let Some(interval) = interval_iter.next() {
                    let index = report_items.len() + 1;
                    report_items.push(json!({
                        "from": interval.start.format("%H:%M").to_string(),
                        "index": index,
                        "result": "",
                        "task": vec![task.clone()].format(),
                        "time": "",
                        "to": interval.end.format("%H:%M").to_string(),
                        "total_ts": format_duration(&interval.duration)
                    }));
                }
            }
        }
    }

    json!(report_items)
}

/// Reads the application configuration and returns an initialized `Si` service instance.
fn get_si_service() -> Result<Si, Box<dyn Error>> {
    Config::read()?
        .si
        .map(|si_config| Si::new(&si_config))
        .ok_or_else(|| "SiServer configuration not found in config file.".into())
}
