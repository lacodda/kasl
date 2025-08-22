//! Daily and monthly report generation and submission command.
//!
//! Handles the core reporting functionality of kasl including generation of detailed
//! daily work reports, automatic filtering of short work intervals, integration with
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


/// Fetches data and displays a formatted daily report in the terminal.
///
/// This function generates a comprehensive daily work report including:
/// - Work intervals with start/end times and durations (filtered by min_work_interval)
/// - Productivity calculations based on actual work vs. presence time
/// - Task completion summary
/// - Break analysis and total pause time
/// - Information about filtered short intervals
///
/// ## Report Components
///
/// 1. **Work Intervals Table**: Shows continuous work periods with breaks (short intervals filtered out)
/// 2. **Summary Statistics**: Total hours, productivity percentage
/// 3. **Task List**: Completed tasks with progress indicators
/// 4. **Filter Information**: Details about intervals filtered due to being too short
///
/// ## Interval Filtering
///
/// Short intervals are automatically filtered from display based on the
/// `min_work_interval` configuration setting. Users are informed about
/// the number and total duration of filtered intervals.
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
    let monitor_config = config.monitor.as_ref().cloned().unwrap_or_default();

    // Fetch filtered long breaks for display (removes noise from short interruptions)
    let long_breaks = Pauses::new()?.get_daily_pauses(naive_date, monitor_config.min_pause_duration)?;
    // Fetch ALL pauses for accurate productivity calculation
    let all_pauses = Pauses::new()?.get_daily_pauses(naive_date, 0)?;

    // Get manual breaks for enhanced productivity calculation
    let breaks = crate::db::breaks::Breaks::new()?.get_daily_breaks(naive_date)?;
    
    // Calculate work intervals and apply filtering
    let intervals = report::calculate_work_intervals(&workday, &long_breaks);
    let (filtered_intervals, filtered_info) = report::filter_short_intervals(&intervals, monitor_config.min_work_interval);

    // Display the formatted report with filtered intervals
    View::report_with_intervals(&workday, &filtered_intervals, &all_pauses, &tasks)?;

    // Display information about filtered short intervals
    if let Some(info) = filtered_info {
        msg_info!(format!("Filtered out {} short intervals (total: {})", info.count, format_duration(&info.total_duration)));
    }

    // Check productivity and show recommendations if needed
    check_and_show_productivity_recommendations(&workday, &all_pauses, &breaks, &config).await?;

    Ok(())
}

/// Handles the complete process of sending a daily report to external API.
///
/// This function manages the full workflow for daily report submission:
/// 1. **Workday Finalization**: Ensures the workday is properly closed
/// 2. **Data Validation**: Verifies required data is available
/// 3. **Interval Filtering**: Applies min_work_interval filtering to remove short intervals
/// 4. **Report Generation**: Creates JSON payload for API submission using filtered intervals
/// 5. **API Submission**: Sends report to configured external service
/// 6. **Monthly Trigger**: Automatically submits monthly report if needed
///
/// ## Report Payload Structure
///
/// The generated JSON includes:
/// - Work intervals with start/end times and durations (short intervals filtered out)
/// - Task assignments distributed across filtered intervals
/// - Summary statistics and metadata
/// - Formatted time strings for external system compatibility
///
/// ## Interval Filtering
///
/// Same filtering logic as display reports - short intervals are automatically
/// removed based on the `min_work_interval` configuration setting before
/// sending to the external API.
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
    let monitor_config = config.monitor.as_ref().cloned().unwrap_or_default();
    let productivity_config = config.productivity.as_ref().cloned().unwrap_or_default();
    let pauses = Pauses::new()?.get_daily_pauses(naive_date, monitor_config.min_pause_duration)?;
    let all_pauses = Pauses::new()?.get_daily_pauses(naive_date, 0)?; // All pauses for productivity calculation
    let breaks = crate::db::breaks::Breaks::new()?.get_daily_breaks(naive_date)?;

    // Check productivity before allowing report submission
    let current_productivity = report::calculate_productivity_with_breaks(&workday, &all_pauses, &breaks);
    if current_productivity < productivity_config.min_productivity_threshold {
        let needed_minutes = report::calculate_needed_break_duration(
            &workday,
            &all_pauses,
            &breaks,
            productivity_config.min_productivity_threshold,
        );
        
        msg_error!(Message::ProductivityTooLowToSend {
            current: current_productivity,
            threshold: productivity_config.min_productivity_threshold,
            needed_break_minutes: needed_minutes,
        });
        return Ok(());
    }

    // Apply interval filtering for API submission
    let intervals = report::calculate_work_intervals(&workday, &pauses);
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
/// * `tasks` - Mutable reference to tasks for modification during processing
/// * `intervals` - Pre-calculated work intervals (potentially filtered)
///
/// # Returns
///
/// Returns a JSON value containing the structured report payload
/// ready for API submission.
fn build_report_payload(_workday: &Workday, tasks: &mut Vec<Task>, intervals: &[report::WorkInterval]) -> serde_json::Value {

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

/// Checks productivity and shows recommendations for improvement if needed.
///
/// This function implements the core productivity monitoring and recommendation
/// system. It calculates enhanced productivity including manual breaks, compares
/// it against configured thresholds, and provides actionable recommendations
/// when productivity falls below acceptable levels.
///
/// ## Recommendation Logic
///
/// The function only shows recommendations when:
/// 1. Sufficient time has elapsed in the workday (prevents early suggestions)
/// 2. Productivity falls below the configured minimum threshold
/// 3. A meaningful break duration can be calculated to reach the target
///
/// ## User Experience
///
/// Recommendations are displayed prominently with:
/// - Red emoji and colored text for visibility
/// - Specific break duration needed
/// - Ready-to-use commands for quick action
/// - Clear explanation of current vs. target productivity
///
/// # Arguments
///
/// * `workday` - Current workday record for timing analysis
/// * `pauses` - All pause periods for productivity calculation
/// * `breaks` - Existing manual breaks already added
/// * `config` - Application configuration with productivity settings
async fn check_and_show_productivity_recommendations(
    workday: &crate::db::workdays::Workday,
    pauses: &[crate::libs::pause::Pause],
    breaks: &[crate::db::breaks::Break],
    config: &crate::libs::config::Config,
) -> Result<()> {
    // Get productivity configuration with defaults
    let productivity_config = config.productivity.as_ref().cloned().unwrap_or_default();
    
    // Check if enough of the workday has passed to make suggestions
    if !report::should_suggest_productivity_improvements(
        workday,
        productivity_config.workday_hours,
        productivity_config.min_workday_fraction_before_suggest,
    ) {
        return Ok(()); // Too early to suggest improvements
    }
    
    // Calculate current productivity including manual breaks
    let current_productivity = report::calculate_productivity_with_breaks(workday, pauses, breaks);
    
    // Check if productivity is below the minimum threshold
    if current_productivity >= productivity_config.min_productivity_threshold {
        return Ok(()); // Productivity is acceptable
    }
    
    // Calculate needed break duration to reach minimum productivity
    let needed_minutes = report::calculate_needed_break_duration(
        workday,
        pauses,
        breaks,
        productivity_config.min_productivity_threshold,
    );
    
    // Only show recommendations if a meaningful break can help
    if needed_minutes >= productivity_config.min_break_duration
        && needed_minutes <= productivity_config.max_break_duration
    {
        // Show prominent productivity warning with colored text
        msg_error!(Message::LowProductivityWarning {
            current: current_productivity,
            threshold: productivity_config.min_productivity_threshold,
            needed_break_minutes: needed_minutes,
        });
    }
    
    Ok(())
}
