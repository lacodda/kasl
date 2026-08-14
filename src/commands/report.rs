//! Daily and monthly report generation and submission command.
//!
//! Handles the core reporting functionality of kasl including generation of detailed
//! daily work reports, automatic filtering of short work intervals, integration with
//! external APIs, and comprehensive productivity analysis using the centralized
//! Productivity module.

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
        messages::Message,
        productivity::Productivity,
        report,
        task::{FormatTasks, Task, TaskFilter},
        view::View,
    },
    msg_error, msg_error_anyhow, msg_info, msg_print,
};
use anyhow::Result;
use chrono::{DateTime, Duration, Local};
use clap::Args;
use serde_json::json;

/// Command-line arguments for the report command.
///
/// The report command supports multiple operational modes for different
/// reporting scenarios and organizational requirements.
#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Submit the generated daily report to configured API
    ///
    /// When specified, the report will be automatically submitted to the
    /// configured reporting service (typically SiServer) after generation.
    /// This enables integration with organizational time tracking systems.
    #[arg(long, help = "Submit daily report")]
    send: bool,

    /// Generate report for the previous day instead of today
    ///
    /// Useful for:
    /// - Submitting yesterday's report in the morning
    /// - Reviewing completed work sessions
    /// - Batch processing of historical reports
    #[arg(long, short, help = "Generate report for the last day")]
    last: bool,

    /// Submit monthly summary report to configured API
    ///
    /// Generates and submits an aggregate monthly report containing
    /// summary statistics and total work hours. Typically used for
    /// organizational reporting requirements at month-end.
    #[arg(long, help = "Submit monthly report")]
    month: bool,
}

/// Main entry point for the report command.
///
/// Acts as a dispatcher based on the provided arguments, determining the target
/// date and delegating to the appropriate handler for daily, monthly, display,
/// or send actions.
///
/// # Returns
///
/// Returns `Ok(())` on successful report generation or processing,
/// or an error if data retrieval or submission fails.
///
/// ```bash
/// # Display today's report
/// kasl report
///
/// # Submit today's report to API
/// kasl report --send
///
/// # Generate yesterday's report
/// kasl report --last
///
/// # Submit monthly summary
/// kasl report --month
///
/// ```
pub async fn cmd(args: ReportArgs) -> Result<()> {
    let date = determine_report_date(args.last);

    if args.month {
        handle_monthly_report(date).await
    } else {
        handle_daily_report(args.send, date).await
    }
}

/// Determines the target date for report generation.
///
/// Calculates whether to generate a report for today or yesterday
/// based on user preferences. This allows flexible reporting timing
/// to accommodate different organizational workflows.
fn determine_report_date(is_last_day: bool) -> DateTime<Local> {
    if is_last_day { Local::now() - Duration::days(1) } else { Local::now() }
}

/// Handles the logic for daily reports.
///
/// Routes to either display or submission mode based on user preferences.
/// This separation allows for different handling of local viewing versus
/// API integration scenarios.
async fn handle_daily_report(should_send: bool, date: DateTime<Local>) -> Result<()> {
    if should_send {
        send_daily_report(date).await
    } else {
        display_daily_report(date).await
    }
}

/// Handles the submission of monthly summary reports.
///
/// Generates and submits aggregate monthly statistics to the configured
/// reporting API. This is typically used for organizational reporting
/// requirements and payroll integration.
async fn handle_monthly_report(date: DateTime<Local>) -> Result<()> {
    let mut si = get_si_service()?;
    let naive_date = date.date_naive();

    match si.send_monthly(&naive_date).await {
        Ok(status) => {
            if status.is_success() {
                msg_info!(Message::MonthlyReportSent(date.format("%B %-d, %Y").to_string()));
            } else {
                msg_error!(Message::MonthlyReportSendFailed(status.to_string()));
            }
        }
        Err(e) => msg_error!(Message::ErrorSendingMonthlyReport(e.to_string())),
    }

    Ok(())
}

/// Fetches data and displays a formatted daily report in the terminal.
///
/// ## Productivity Calculation
///
/// Productivity is calculated as:
/// ```text
/// Productivity = (Net Work Time / Available Work Time) * 100%
/// Where Available Work Time = Gross Work Time - Manual Breaks - Long Pauses
/// ```
///
/// This provides insight into work efficiency while accounting for
/// legitimate breaks and focusing on actual productive activity.
///
/// # Data Sources
///
/// The report integrates multiple data sources:
async fn display_daily_report(date: DateTime<Local>) -> Result<()> {
    let naive_date = date.date_naive();
    let workday = match Workdays::new()?.fetch(naive_date)? {
        Some(wd) => wd,
        None => {
            msg_print!(Message::WorkdayNotFoundForDate(date.format("%B %-d, %Y").to_string()), true);
            return Ok(());
        }
    };

    let tasks = Tasks::new()?.fetch(TaskFilter::Date(naive_date))?;
    let config = Config::read()?;
    let monitor_config = config.monitor.as_ref().cloned().unwrap_or_default();

    // Load interruptions: detected pauses above the threshold plus any manual
    // pauses the user recorded (protected records bypass the threshold).
    let long_pauses = Pauses::new()?
        .set_min_duration(monitor_config.min_pause_duration)
        .get_workday_pauses(&workday)?;

    // Calculate work intervals and apply filtering
    let intervals = report::calculate_work_intervals(&workday, &long_pauses);
    let (filtered_intervals, filtered_info) = report::filter_short_intervals(&intervals, monitor_config.min_work_interval);

    // Use the report module to process the data
    let (filtered_duration, productivity) = report::report_with_intervals(&workday, &intervals)?;

    // Display the formatted report with filtered intervals
    View::report(&workday, &filtered_intervals, &filtered_duration, &productivity, &tasks)?;

    // Display information about filtered short intervals
    if let Some(info) = filtered_info {
        msg_info!(format!(
            "Filtered out {} short intervals (total: {})",
            info.count,
            format_duration(&info.total_duration)
        ));
    }

    // Warn when productivity is below the configured threshold
    let productivity = Productivity::new(&workday)?;
    if productivity.is_below_threshold() {
        msg_error!(Message::LowProductivityWarning {
            current: productivity.calculate_productivity(),
            threshold: productivity.config.min_productivity_threshold,
        });
    }

    Ok(())
}

/// Handles the complete process of sending a daily report to external API.
async fn send_daily_report(date: DateTime<Local>) -> Result<()> {
    let naive_date = date.date_naive();
    let mut workdays_db = Workdays::new()?;

    // Finalize the workday by recording end time
    workdays_db.insert_end(naive_date)?;

    // Load the finalized workday data
    let workday = workdays_db
        .fetch(naive_date)?
        .ok_or_else(|| msg_error_anyhow!(Message::WorkdayCouldNotFindAfterFinalizing(naive_date.to_string())))?;

    // Validate that tasks exist for the reporting day
    let mut tasks = Tasks::new()?.fetch(TaskFilter::Date(naive_date))?;
    if tasks.is_empty() {
        msg_error!(Message::TasksNotFoundForDate(date.format("%B %-d, %Y").to_string()));
        return Ok(());
    }

    let config = Config::read()?;
    let monitor_config = config.monitor.as_ref().cloned().unwrap_or_default();

    // Load interruptions for the submitted intervals: detected pauses above the
    // threshold plus any manual pauses the user recorded.
    let long_pauses = Pauses::new()?
        .set_min_duration(monitor_config.min_pause_duration)
        .get_workday_pauses(&workday)?;

    // Validate productivity before allowing report submission
    // Uses centralized Productivity module for consistent threshold checking
    let productivity = Productivity::new(&workday)?;
    let current_productivity = productivity.calculate_productivity();
    if current_productivity < productivity.config.min_productivity_threshold {
        msg_error!(Message::ProductivityTooLowToSend {
            current: current_productivity,
            threshold: productivity.config.min_productivity_threshold,
        });
        return Ok(());
    }

    // Apply interval filtering for API submission
    let intervals = report::calculate_work_intervals(&workday, &long_pauses);
    let (filtered_intervals, _) = report::filter_short_intervals(&intervals, monitor_config.min_work_interval);

    // Generate JSON payload for API submission using filtered intervals
    let report_json = build_report_payload(&workday, &mut tasks, &filtered_intervals);
    let events_json = serde_json::to_string(&report_json)?;
    let mut si = get_si_service()?;

    // Submit the report to external API
    match si.send(&events_json, &naive_date).await {
        Ok(status) => {
            if status.is_success() {
                msg_info!(Message::DailyReportSent(date.format("%B %-d, %Y").to_string()));

                // Check if monthly report should be automatically triggered
                if si.is_last_working_day_of_month(&naive_date)? {
                    msg_info!(Message::MonthlyReportTriggered);
                    handle_monthly_report(date).await?;
                }
            } else {
                msg_error!(Message::ReportSendFailed(status.to_string()));
            }
        }
        Err(e) => msg_error!(Message::ErrorSendingEvents(e.to_string())),
    }

    Ok(())
}

/// Builds the JSON payload for API submission.
fn build_report_payload(_workday: &Workday, tasks: &mut [Task], intervals: &[report::WorkInterval]) -> serde_json::Value {
    let num_tasks = tasks.len();
    let num_intervals = intervals.len();

    // Handle edge case of no work intervals
    if num_intervals == 0 {
        return json!([]);
    }

    let mut report_items = Vec::new();

    // Distribute tasks across intervals based on relative quantities
    if num_tasks >= num_intervals {
        // More tasks than intervals: multiple tasks per interval
        let mut task_iter = tasks.iter();
        let base_tasks_per_interval = num_tasks / num_intervals;
        let mut extra_tasks = num_tasks % num_intervals;

        for (i, interval) in intervals.iter().enumerate() {
            // Calculate number of tasks for this interval
            let count = base_tasks_per_interval + if extra_tasks > 0 { 1 } else { 0 };
            extra_tasks = extra_tasks.saturating_sub(1);

            // Collect tasks for this interval
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
        // More intervals than tasks: multiple intervals per task
        let mut interval_iter = intervals.iter();
        let base_intervals_per_task = num_intervals / num_tasks;
        let mut extra_intervals = num_intervals % num_tasks;

        for task in tasks.iter() {
            // Calculate number of intervals for this task
            let count = base_intervals_per_task + if extra_intervals > 0 { 1 } else { 0 };
            extra_intervals = extra_intervals.saturating_sub(1);

            // Create entries for each interval assigned to this task
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

/// Reads configuration and returns an initialized Si service instance.
///
/// This helper function encapsulates the configuration loading and service
/// initialization logic, providing proper error handling for missing or
/// invalid SiServer configuration.
fn get_si_service() -> Result<Si> {
    Config::read()?
        .si
        .map(|si_config| Si::new(&si_config))
        .ok_or_else(|| msg_error_anyhow!(Message::SiServerConfigNotFound))
}
