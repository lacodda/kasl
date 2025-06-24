use crate::{
    api::si::Si,
    db::{breaks::Breaks, tasks::Tasks, workdays::Workdays},
    libs::{
        config::Config,
        task::{FormatTasks, TaskFilter},
        view::View,
    },
};
use chrono::{Duration, Local};
use clap::Args;
use std::error::Error;

/// Command-line arguments for the `report` command.
#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Submits the report to the configured API.
    #[arg(long, help = "Submit report")]
    send: bool,
    /// Generates a report for the previous day.
    #[arg(long, short, help = "Last day report")]
    last: bool,
    /// Submits a monthly summary report.
    #[arg(long, help = "Submit monthly report")]
    month: bool,
}

/// Executes the `report` command to generate or submit a work report.
///
/// Handles three modes:
/// - Displays a daily report with work intervals and tasks if no flags are provided.
/// - Submits a daily report to the SiServer API if `--send` is used.
/// - Submits a monthly report if `--month` is used.
/// Supports `--last` to generate a report for the previous day.
///
/// # Arguments
/// * `report_args` - The parsed command-line arguments.
///
/// # Returns
/// A `Result` indicating success or an error if database or API operations fail.
pub async fn cmd(report_args: ReportArgs) -> Result<(), Box<dyn Error>> {
    // Determine the report date (today or yesterday if --last is specified).
    let mut date = Local::now();
    if report_args.last {
        date = date - Duration::days(1);
    }
    let naive_date = date.date_naive();

    // Handle monthly report submission.
    if report_args.month {
        match Config::read()?.si {
            Some(si_config) => {
                let mut si = Si::new(&si_config);
                let monthly_status = si.send_monthly(&naive_date).await?;
                if monthly_status.is_success() {
                    println!(
                        "Your monthly report dated {} has been successfully submitted\nWait for a message to your email address",
                        date.format("%B %-d, %Y")
                    );
                } else {
                    println!("Failed to send monthly report. Status: {}", monthly_status);
                }
            }
            None => eprintln!("Failed to read SiServer config"),
        }
        return Ok(());
    }

    // Finalize the workday if submitting a report.
    if report_args.send {
        Workdays::new()?.insert_end(naive_date)?;
    }

    // Fetch the workday record for the specified date.
    let workday = match Workdays::new()?.fetch(naive_date)? {
        Some(wd) => wd,
        None => {
            println!("\nNo workday record found for {}", date.format("%B %-d, %Y"));
            return Ok(());
        }
    };

    // Fetch tasks for the specified date.
    let mut tasks = Tasks::new()?.fetch(TaskFilter::Date(naive_date))?;

    // Handle report submission to the SiServer API.
    if report_args.send {
        if tasks.is_empty() {
            println!("Tasks not found for {}, report not sent.", date.format("%B %-d, %Y"));
            return Ok(());
        }

        let workday_end = workday.end.unwrap_or_else(|| Local::now().naive_local());
        let report_json = serde_json::json!([{
            "from": workday.start.format("%H:%M").to_string(),
            "to": workday_end.format("%H:%M").to_string(),
            "total_ts": (workday_end - workday.start).num_seconds(),
            "task": tasks.format(),
            "data": [],
            "time": "",
            "result": ""
        }]);

        let events_json = serde_json::to_string(&report_json)?;

        match Config::read()?.si {
            Some(si_config) => {
                let mut si = Si::new(&si_config);
                match si.send(&events_json, &naive_date).await {
                    Ok(status) => {
                        if status.is_success() {
                            println!(
                                "Your report dated {} has been successfully submitted\nWait for a message to your email address",
                                date.format("%B %-d, %Y")
                            );
                            if si.is_last_working_day_of_month(&naive_date)? {
                                let monthly_status = si.send_monthly(&naive_date).await?;
                                if monthly_status.is_success() {
                                    println!("Your monthly report has also been successfully submitted.");
                                }
                            }
                        } else {
                            println!("Status: {}", status);
                        }
                    }
                    Err(e) => eprintln!("Error sending events: {}", e),
                }
            }
            None => eprintln!("Failed to read SiServer config"),
        }
        return Ok(());
    }

    // Fetch breaks for the specified date using the configured minimum break duration.
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();
    let breaks = Breaks::new()?.fetch(naive_date, monitor_config.min_break_duration)?;

    // Display the report with work intervals and tasks.
    View::report(&workday, &breaks, &tasks)?;

    Ok(())
}
