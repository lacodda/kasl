//! Daily and monthly report generation and submission command.
//!
//! Handles the core reporting functionality of kasl including generation of detailed
//! daily work reports, automatic filtering of short work intervals, integration with
//! external APIs, and comprehensive productivity analysis using the centralized
//! Productivity module.

use crate::{
    api::si::{Si, daily_report_fields},
    db::{
        tasks::Tasks,
        workdays::{Workday, Workdays},
    },
    libs::{
        config::Config,
        day_end::{self, Closed},
        formatter::format_duration,
        json,
        messages::Message,
        output::{self, Format},
        plugin::{events, manifest::Event},
        report::{self, DayReport, DayWork},
        task::{FormatTasks, Task, TaskFilter},
        view::View,
    },
    msg_error, msg_error_anyhow, msg_info, msg_print,
};
use anyhow::{Result, bail};
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
    #[arg(long, help = "Submit monthly report", conflicts_with = "json")]
    month: bool,

    /// Print the payload before it is sent, and send nothing
    ///
    /// The same transparency `kasl server manifest` gives for the team
    /// server, for the corporate channel: what leaves this machine, in full,
    /// before it does. Requires `--send`, because what it describes is the
    /// sending.
    ///
    /// Conflicts with `--month` rather than growing a monthly preview: the
    /// monthly report sends a date and nothing else, so there is no payload
    /// to inspect, and accepting the flag there would answer a question about
    /// the daily payload with silence.
    #[arg(
        long,
        requires = "send",
        conflicts_with = "month",
        help = "Show the payload that --send would post, and post nothing"
    )]
    show: bool,

    /// The day as JSON - or, with `--send --show`, the payload as JSON
    #[command(flatten)]
    format: Format,
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
    let json = args.format.json;

    if args.show {
        show_daily_payload(date, json).await
    } else if args.month {
        handle_monthly_report(date).await
    } else if args.send && json {
        // Sending prints what happened to the request, not data; the payload
        // it would post is what `--send --show --json` prints.
        bail!("--json prints the day, or with --send --show the payload; sending itself has no JSON form")
    } else if args.send {
        send_daily_report(date).await
    } else {
        display_daily_report(date, json)
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
fn display_daily_report(date: DateTime<Local>, json: bool) -> Result<()> {
    let day = DayReport::read(date.date_naive())?;
    if json {
        return output::print_json(&day.to_json());
    }

    let Some(workday) = &day.workday else {
        msg_print!(Message::WorkdayNotFoundForDate(date.format("%B %-d, %Y").to_string()), true);
        return Ok(());
    };

    View::report(workday, &day.intervals, &day.work, &day.productivity.unwrap_or_default(), &day.tasks)?;

    // Information about filtered short intervals
    if let Some(info) = &day.dropped {
        msg_info!(format!(
            "Filtered out {} short intervals (total: {})",
            info.count,
            format_duration(&info.total_duration)
        ));
    }

    // What is still waiting in the inbox - the day is not only what was done.
    if !day.inbox.is_empty() {
        msg_info!(Message::InboxSummary {
            total: day.inbox.total,
            fresh: day.inbox.fresh,
            taken: day.inbox.taken,
        });
    }

    // Warn when productivity is below the configured threshold
    if let (true, Some(current), Some(threshold)) = (day.below_threshold, day.productivity, day.threshold) {
        msg_error!(Message::LowProductivityWarning { current, threshold });
    }

    Ok(())
}

/// Prints the payload `--send` would post, and posts nothing.
///
/// The point is being able to read what leaves this machine before it does.
/// That only means something if the preview is the real payload, so this
/// builds it through the same functions the send path uses - the same
/// interval filtering, the same task distribution, the same form fields from
/// [`daily_report_fields`] - and stops one step short of the request.
///
/// Three things the send path does are deliberately *not* done here.
///
/// The day is not finalized. `--send` writes an end timestamp before it
/// assembles anything, and a preview that quietly ended someone's working day
/// would be the opposite of a command you run to find out what would happen.
/// The day is read as it stands, open end and all.
///
/// The productivity threshold is not enforced. It decides whether a report
/// may be submitted, not what the submission contains, and refusing to show
/// the payload of a day that is below it would hide exactly the day someone
/// wants to look at.
///
/// Nothing is authenticated. No session is opened, no credential is read: the
/// address comes from the config, and a preview that logged in would reach
/// the network to tell you what it would do if it reached the network.
///
/// With `--json` a day that cannot be sent is an error rather than a message:
/// there is no payload to print, and an empty stdout with exit 0 would read
/// as one that sends nothing.
async fn show_daily_payload(date: DateTime<Local>, json: bool) -> Result<()> {
    let naive_date = date.date_naive();

    let workday = match Workdays::new()?.fetch(naive_date)? {
        Some(workday) => workday,
        None if json => return Err(msg_error_anyhow!(Message::WorkdayNotFoundForDate(date.format("%B %-d, %Y").to_string()))),
        None => {
            msg_print!(Message::WorkdayNotFoundForDate(date.format("%B %-d, %Y").to_string()), true);
            return Ok(());
        }
    };

    let mut tasks = Tasks::new()?.fetch(TaskFilter::Date(naive_date))?;
    if tasks.is_empty() {
        // The same stop `--send` makes, and for the same reason: there is no
        // report to describe. Reported rather than shown as an empty payload,
        // which would read as "this day sends nothing" when what it means is
        // "this day cannot be sent".
        if json {
            return Err(msg_error_anyhow!(Message::TasksNotFoundForDate(date.format("%B %-d, %Y").to_string())));
        }
        msg_error!(Message::TasksNotFoundForDate(date.format("%B %-d, %Y").to_string()));
        return Ok(());
    }

    // The intervals the report shows are the ones that would be sent.
    let day = DayWork::of(&workday, &Config::read()?.monitor.unwrap_or_default())?;
    let report_json = build_report_payload(&workday, &mut tasks, &day.intervals);
    let events_json = serde_json::to_string(&report_json)?;

    // The plugins are the other half of where the day goes: each gets the day
    // as `kasl report --json` prints it, and is named here before it does.
    let plugins: Vec<String> = events::listeners(Event::ReportReady).into_iter().map(|plugin| plugin.name).collect();
    if !plugins.is_empty() && !json {
        msg_info!(Message::ReportGoesToPlugins(plugins.join(", ")));
    }

    // The destination is half the answer to "what leaves this machine", so it
    // is named from the configured client rather than described in prose.
    // Without SiServer the day may still go to plugins: then that is the
    // whole answer, and nothing is posted anywhere else.
    let Some(si) = Config::read()?.si.map(|config| Si::new(&config)) else {
        if plugins.is_empty() {
            return Err(msg_error_anyhow!(Message::ReportHasNowhereToGo));
        }
        if json {
            return output::print_json(&json::Payload {
                url: None,
                date: naive_date,
                fields: Vec::new(),
                tasks: serde_json::Value::Null,
                plugins,
            });
        }
        msg_info!(Message::ReportNotPostedToSiServer);
        return Ok(());
    };
    let fields = daily_report_fields(&naive_date.format("%Y-%m-%d").to_string(), &events_json);
    if json {
        return output::print_json(&json::Payload {
            url: Some(si.daily_report_url()),
            date: naive_date,
            fields: fields
                .into_iter()
                .map(|(name, value)| json::Field {
                    name: name.to_string(),
                    value: value.to_string(),
                })
                .collect(),
            tasks: report_json,
            plugins,
        });
    }
    msg_info!(Message::ReportPayloadHeading {
        url: si.daily_report_url(),
        date: naive_date.to_string(),
    });

    // Every field of the form, not only the interesting one. `tasks` is where
    // the work is described, but a person asking what is sent about them is
    // owed the whole request - a field they were not shown is a field they
    // were not told about.
    for (name, value) in fields {
        // The tasks field holds the report as JSON; pretty-printing it is the
        // difference between a payload someone can read and one they can only
        // confirm exists.
        if name == "tasks" {
            msg_print!(format!("  {}:", name));
            // Indented one step further than the field lines. The JSON is the
            // only multi-line value here, and without a shift its own braces
            // sit in the same column as a field name - readable enough to a
            // person, and indistinguishable to anything reading the preview
            // back, which the parity check against the real request does.
            for line in serde_json::to_string_pretty(&report_json)?.lines() {
                msg_print!(format!("    {}", line));
            }
        } else if value.is_empty() {
            // Shown as empty rather than skipped: a field sent empty is still
            // a field sent, and leaving it out would describe a smaller
            // request than the one made.
            msg_print!(format!("  {}: (empty)", name));
        } else {
            msg_print!(format!("  {}: {}", name, value));
        }
    }

    Ok(())
}

/// Sends the day to every channel it goes to: SiServer, and the plugins that hear `report_ready`.
///
/// The channels are found first, before anything is written: a send with
/// nowhere to go must not close the day on its way to saying so. Then the
/// day is closed by the one rule a day's end follows ([`day_end`]) - a past
/// day keeps the end it has - and checked: tasks there, productivity over
/// the minimum. A day that passes is ready, and each channel gets it on its
/// own: SiServer failing does not keep the plugins from hearing, nor the
/// other way round.
async fn send_daily_report(date: DateTime<Local>) -> Result<()> {
    let naive_date = date.date_naive();
    let si_config = Config::read()?.si;
    let listeners = events::listeners(Event::ReportReady);
    if si_config.is_none() && listeners.is_empty() {
        return Err(msg_error_anyhow!(Message::ReportHasNowhereToGo));
    }
    let mut workdays_db = Workdays::new()?;

    // A day that was never started cannot be sent, and saying so here beats
    // the fetch below failing with "could not find after finalizing".
    match day_end::close(naive_date)? {
        Closed::NeverStarted => return Err(msg_error_anyhow!(Message::WorkdayNeverStarted(naive_date.to_string()))),
        Closed::Ended(_) => day_end::tell(naive_date),
        Closed::Kept(_) => {}
    }

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

    // The day as the report counts it: intervals between long pauses, short
    // ones left out, productivity against the configured minimum.
    let day = DayWork::of(&workday, &Config::read()?.monitor.unwrap_or_default())?;
    if day.productivity < day.threshold {
        msg_error!(Message::ProductivityTooLowToSend {
            current: day.productivity,
            threshold: day.threshold,
        });
        return Ok(());
    }

    if let Some(si_config) = si_config {
        let report_json = build_report_payload(&workday, &mut tasks, &day.intervals);
        let events_json = serde_json::to_string(&report_json)?;
        let mut si = Si::new(&si_config);

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
    }

    if !listeners.is_empty() {
        let subject = day_end::day_document(naive_date)?;
        let deliveries = tokio::task::spawn_blocking(move || events::fire_to(&listeners, Event::ReportReady, &subject))
            .await
            .map_err(|error| anyhow::anyhow!("handing the day to plugins stopped: {error}"))?;
        events::announce(Event::ReportReady, &deliveries);
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
