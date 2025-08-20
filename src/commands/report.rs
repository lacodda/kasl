//! Daily and monthly report generation and submission command.
//!
//! Handles the core reporting functionality of kasl including generation of detailed
//! daily work reports, automatic cleanup of short work intervals, integration with
//! external APIs, and productivity analysis.

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
        pause::Pause,
        report::{self, analyze_short_intervals},
        task::{FormatTasks, Task, TaskFilter},
        view::View,
    },
    msg_error, msg_error_anyhow, msg_info, msg_print, msg_success, msg_warning,
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

    /// Automatically detect and remove short work intervals
    ///
    /// Analyzes work intervals and removes pauses that create
    /// inappropriately short work periods, merging adjacent
    /// intervals for cleaner reporting. This helps eliminate
    /// noise from brief interruptions.
    #[arg(long, short, help = "Clear short work intervals automatically")]
    clear_short_intervals: bool,
}

/// Main entry point for the report command.
///
/// Acts as a dispatcher based on the provided arguments, determining the target
/// date and delegating to the appropriate handler for daily, monthly, display,
/// or send actions.
///
/// # Arguments
///
/// * `args` - Parsed command-line arguments specifying report options
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
/// # Clean up short intervals and show updated report
/// kasl report --clear-short-intervals
/// ```
pub async fn cmd(args: ReportArgs) -> Result<()> {
    let date = determine_report_date(args.last);

    if args.clear_short_intervals {
        return handle_clear_short_intervals(date).await;
    }

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
///
/// # Arguments
///
/// * `is_last_day` - Whether to generate report for yesterday
///
/// # Returns
///
/// Returns the target date with timezone information for report generation.
fn determine_report_date(is_last_day: bool) -> DateTime<Local> {
    if is_last_day {
        Local::now() - Duration::days(1)
    } else {
        Local::now()
    }
}

/// Handles the logic for daily reports.
///
/// Routes to either display or submission mode based on user preferences.
/// This separation allows for different handling of local viewing versus
/// API integration scenarios.
///
/// # Arguments
///
/// * `should_send` - Whether to submit the report to external API
/// * `date` - Target date for report generation
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
///
/// ## Monthly Report Contents
///
/// - Total hours worked in the month
/// - Number of working days
/// - Average daily hours
/// - Productivity trends (if available)
///
/// # Arguments
///
/// * `date` - Date within the target month for report generation
///
/// # Error Handling
///
/// Network errors are handled gracefully with user-friendly messages
/// rather than application crashes, allowing continued local operation
/// even when API services are unavailable.
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

/// Handles automatic detection and cleanup of short work intervals.
///
/// This function analyzes work patterns to identify and remove inappropriately
/// short work intervals that result from brief interruptions or system noise.
/// It helps create cleaner, more accurate reports by merging adjacent work
/// periods separated by very brief pauses.
///
/// ## Short Interval Detection
///
/// The algorithm:
/// 1. **Interval Calculation**: Determines work periods between pauses
/// 2. **Duration Analysis**: Identifies intervals shorter than configured minimum
/// 3. **Pause Removal**: Removes pauses that create short intervals
/// 4. **Interval Merging**: Combines adjacent work periods for cleaner reporting
///
/// ## Configuration
///
/// Short interval threshold is controlled by `min_work_interval` setting
/// in the monitor configuration. Common values:
/// - 5 minutes: Aggressive cleanup, removes most brief interruptions
/// - 10 minutes: Moderate cleanup, preserves intentional short tasks
/// - 15+ minutes: Conservative cleanup, only removes obvious noise
///
/// # Arguments
///
/// * `date` - Target date for interval analysis and cleanup
///
/// # Returns
///
/// Returns `Ok(())` after completing cleanup and displaying updated report.
/// Shows detailed information about removed intervals and updated statistics.
async fn handle_clear_short_intervals(date: DateTime<Local>) -> Result<()> {
    let naive_date = date.date_naive();
    let workday = match Workdays::new()?.fetch(naive_date)? {
        Some(wd) => wd,
        None => {
            msg_error!(Message::WorkdayNotFoundForDate(date.format("%B %-d, %Y").to_string()));
            return Ok(());
        }
    };

    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();

    let pauses_db = Pauses::new()?;
    let all_pauses = pauses_db.get_daily_pauses(naive_date, 0)?; // Fetch all pauses including short ones

    // Calculate work intervals to analyze for short periods
    let intervals = report::calculate_work_intervals(&workday, &all_pauses);

    // Analyze intervals to identify short ones for removal
    if let Some(short_info) = analyze_short_intervals(&intervals, monitor_config.min_work_interval) {
        msg_print!(Message::ShortIntervalsToRemove(short_info.count), true);

        // Display detailed information about intervals to be removed
        for (_idx, interval) in &short_info.intervals {
            println!(
                "  • {} - {} ({})",
                interval.start.format("%H:%M"),
                interval.end.format("%H:%M"),
                format_duration(&interval.duration)
            );
        }

        // Extract pause IDs for database removal
        let pause_ids: Vec<i32> = short_info
            .pauses_to_remove
            .iter()
            .filter_map(|&idx| all_pauses.get(idx).and_then(|p| Some(p.id)))
            .collect();

        if !pause_ids.is_empty() {
            msg_info!(Message::RemovingPauses(pause_ids.len()));
            let deleted = pauses_db.delete_many(&pause_ids)?;
            msg_success!(Message::ShortIntervalsCleared(deleted));

            // Display updated report after cleanup
            msg_print!(Message::UpdatedReport, true);
            display_daily_report(date).await?;
        } else {
            msg_warning!(Message::NoRemovablePausesFound);
        }
    } else {
        msg_info!(Message::NoShortIntervalsFound(monitor_config.min_work_interval));
    }

    Ok(())
}

/// Fetches data and displays a formatted daily report in the terminal.
///
/// This function generates a comprehensive daily work report including:
/// - Work intervals with start/end times and durations
/// - Productivity calculations based on actual work vs. presence time
/// - Task completion summary
/// - Break analysis and total pause time
/// - Short interval detection warnings
///
/// ## Report Components
///
/// 1. **Work Intervals Table**: Shows continuous work periods with breaks
/// 2. **Summary Statistics**: Total hours, productivity percentage
/// 3. **Task List**: Completed tasks with progress indicators
/// 4. **Data Quality Warnings**: Alerts about short intervals or data issues
///
/// ## Productivity Calculation
///
/// Productivity is calculated as:
/// ```
/// Productivity = (Net Work Time / Gross Work Time - Long Breaks) * 100%
/// ```
///
/// This provides insight into work efficiency while accounting for
/// legitimate breaks and focusing on actual productive activity.
///
/// # Arguments
///
/// * `date` - Target date for report generation
///
/// # Data Sources
///
/// The report integrates multiple data sources:
/// - **Workdays**: Start and end times for the work session
/// - **Pauses**: Automatically detected breaks and manual pauses
/// - **Tasks**: Completed work items and progress tracking
/// - **Configuration**: Thresholds for filtering and analysis
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
    let monitor_config = config.monitor.unwrap_or_default();

    // Fetch filtered long breaks for display (removes noise from short interruptions)
    let long_breaks = Pauses::new()?.get_daily_pauses(naive_date, monitor_config.min_pause_duration)?;
    // Fetch ALL pauses for accurate productivity calculation
    let all_pauses = Pauses::new()?.get_daily_pauses(naive_date, 0)?;

    // Display the formatted report with all components
    View::report(&workday, &long_breaks, &all_pauses, &tasks)?;

    // Analyze for short intervals and provide user guidance
    let intervals = report::calculate_work_intervals(&workday, &long_breaks);
    if let Some(short_info) = analyze_short_intervals(&intervals, monitor_config.min_work_interval) {
        msg_warning!(Message::ShortIntervalsDetected(short_info.count, format_duration(&short_info.total_duration)));
        msg_info!(Message::UseReportClearCommand);
    }

    Ok(())
}

/// Handles the complete process of sending a daily report to external API.
///
/// This function manages the full workflow for daily report submission:
/// 1. **Workday Finalization**: Ensures the workday is properly closed
/// 2. **Data Validation**: Verifies required data is available
/// 3. **Report Generation**: Creates JSON payload for API submission
/// 4. **API Submission**: Sends report to configured external service
/// 5. **Monthly Trigger**: Automatically submits monthly report if needed
///
/// ## Report Payload Structure
///
/// The generated JSON includes:
/// - Work intervals with start/end times and durations
/// - Task assignments distributed across intervals
/// - Summary statistics and metadata
/// - Formatted time strings for external system compatibility
///
/// ## Auto-Monthly Reporting
///
/// If the current date is the last working day of the month,
/// this function will automatically trigger monthly report submission
/// after successful daily report processing.
///
/// # Arguments
///
/// * `date` - Target date for report generation and submission
///
/// # Error Handling
///
/// The function handles several error scenarios gracefully:
/// - Missing workday data (warns user, doesn't crash)
/// - No tasks for the day (prevents submission, shows warning)
/// - Network connectivity issues (reports error, continues operation)
/// - API authentication failures (provides user-friendly messages)
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
    let monitor_config = config.monitor.unwrap_or_default();
    let pauses = Pauses::new()?.get_daily_pauses(naive_date, monitor_config.min_pause_duration)?;

    // Generate JSON payload for API submission
    let report_json = build_report_payload(&workday, &mut tasks, &pauses);
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
///
/// This function creates a structured JSON report that distributes tasks
/// across work intervals in a logical manner. The distribution algorithm
/// ensures that all tasks are included and work intervals are properly
/// represented in the external reporting system.
///
/// ## Task Distribution Algorithm
///
/// The function handles two scenarios:
///
/// 1. **More Tasks than Intervals**: Distributes multiple tasks per interval
///    - Calculates base tasks per interval
///    - Distributes remainder tasks evenly
///    - Ensures all tasks are included
///
/// 2. **More Intervals than Tasks**: Assigns intervals to tasks
///    - Distributes multiple intervals per task
///    - Creates separate entries for each interval
///    - Maintains interval granularity
///
/// ## JSON Structure
///
/// Each report entry contains:
/// - `from`: Start time in HH:MM format
/// - `to`: End time in HH:MM format
/// - `total_ts`: Formatted duration string
/// - `task`: Formatted task description with completion percentage
/// - `index`: Sequential numbering for external system ordering
/// - `result`: Empty field for external system use
/// - `time`: Empty field for external system use
///
/// # Arguments
///
/// * `workday` - The workday record with start/end times
/// * `tasks` - Mutable reference to tasks for modification during processing
/// * `pauses` - Pause records for interval calculation
///
/// # Returns
///
/// Returns a JSON value containing the structured report payload
/// ready for API submission.
fn build_report_payload(workday: &Workday, tasks: &mut Vec<Task>, pauses: &[Pause]) -> serde_json::Value {
    // Calculate work intervals based on workday and pause data
    let intervals = report::calculate_work_intervals(workday, pauses);

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
            if extra_tasks > 0 {
                extra_tasks -= 1;
            }

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
            if extra_intervals > 0 {
                extra_intervals -= 1;
            }

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
///
/// # Returns
///
/// Returns a configured Si service instance ready for API operations,
/// or an error if SiServer configuration is missing or invalid.
///
/// # Error Scenarios
///
/// - Configuration file not found or unreadable
/// - SiServer section missing from configuration
/// - Invalid API credentials or URLs in configuration
fn get_si_service() -> Result<Si> {
    Config::read()?
        .si
        .map(|si_config| Si::new(&si_config))
        .ok_or_else(|| msg_error_anyhow!(Message::SiServerConfigNotFound))
}
