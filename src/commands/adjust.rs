//! Workday time adjustment command.
//!
//! This command provides manual correction capabilities for automatically recorded
//! work times. It supports removing time from the beginning or end of workdays
//! and adding manual pauses that weren't detected by the monitoring system.

use crate::{
    db::{pauses::Pauses, workdays::Workdays},
    libs::{formatter::format_duration, messages::Message},
    msg_bail_anyhow, msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use clap::{Args, ValueEnum};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

/// Available adjustment modes for modifying workday times.
///
/// Each mode provides a different type of time correction:
/// - **Start**: Remove time from the beginning of the workday
/// - **End**: Remove time from the end of the workday  
/// - **Pause**: Add a manual pause within the workday
#[derive(Debug, Clone, ValueEnum)]
enum AdjustmentMode {
    /// Remove time from the start of the workday
    ///
    /// This mode shifts the workday start time forward, effectively removing
    /// the specified duration from the beginning of the work session. Useful for:
    /// - Correcting early false starts from the monitor
    /// - Accounting for personal time before actual work began
    /// - Adjusting for system wake-up activity
    Start,

    /// Remove time from the end of the workday
    ///
    /// This mode shifts the workday end time backward, removing the specified
    /// duration from the end of the work session. Useful for:
    /// - Correcting late false activity from system processes
    /// - Removing personal time after work ended
    /// - Adjusting for shutdown activity
    End,

    /// Add a pause in the middle of the workday
    ///
    /// This mode inserts a manual pause at a specified time within the workday.
    /// The pause duration is subtracted from the total work time. Useful for:
    /// - Adding breaks that weren't automatically detected
    /// - Recording meetings or phone calls away from the computer
    /// - Accounting for manual activities not detected by monitoring
    Pause,
}

/// Command-line arguments for workday time adjustments.
///
/// The adjust command supports both interactive and non-interactive operation,
/// allowing users to specify all parameters via command line or be prompted
/// for missing information.
#[derive(Debug, Args)]
pub struct AdjustArgs {
    /// Date to adjust workday for
    ///
    /// Accepts either 'today' for the current date or a specific date in
    /// 'YYYY-MM-DD' format. This allows correction of any historical workday.
    #[arg(long, short, default_value = "today")]
    date: String,

    /// Minutes to subtract or pause duration to add
    ///
    /// The interpretation of this value depends on the adjustment mode:
    /// - **Start/End modes**: Minutes to remove from workday
    /// - **Pause mode**: Duration of the pause to add
    ///
    /// If not specified, the user will be prompted interactively.
    #[arg(long, short)]
    minutes: Option<u64>,

    /// Type of adjustment to perform
    ///
    /// Specifies which adjustment operation to perform. If not provided,
    /// the user will be prompted to select from available options.
    #[arg(long, value_enum)]
    mode: Option<AdjustmentMode>,

    /// Skip confirmation prompt and apply changes immediately
    ///
    /// When specified, changes will be applied without showing a preview
    /// or asking for confirmation. Use with caution as adjustments modify
    /// the permanent work record.
    #[arg(long)]
    force: bool,
}

/// Executes the workday time adjustment command.
///
/// This function handles the complete adjustment workflow:
/// 1. **Validation**: Ensures a workday exists for the specified date
/// 2. **Parameter Collection**: Gathers adjustment mode and duration (interactively if needed)
/// 3. **Preview Generation**: Shows the current state and proposed changes
/// 4. **Confirmation**: Requests user approval (unless `--force` is used)
/// 5. **Application**: Applies the changes to the database
///
/// ## Safety Features
///
/// The adjustment process includes several safety measures:
/// - **Preview Display**: Shows before/after comparison before applying changes
/// - **Validation**: Ensures adjustments won't create invalid workdays
/// - **Confirmation**: Requires user approval for all changes (unless forced)
/// - **Bounds Checking**: Prevents adjustments that would exceed workday limits
///
/// ## Interactive Mode
///
/// When parameters are not provided via command line, the function will:
/// - Prompt for adjustment mode selection with descriptive options
/// - Request duration input with validation
/// - For pause mode, additionally prompt for the pause start time
///
/// # Arguments
///
/// * `args` - Parsed command-line arguments containing adjustment parameters
///
/// # Returns
///
/// Returns `Ok(())` on successful adjustment or user cancellation, or an error
/// if the operation fails due to invalid parameters or database issues.
///
/// # Examples
///
/// ```bash
/// # Interactive adjustment for today
/// kasl adjust
///
/// # Remove 30 minutes from start of workday
/// kasl adjust --mode start --minutes 30
///
/// # Add a 1-hour lunch break at 12:00
/// kasl adjust --mode pause --minutes 60
///
/// # Adjust specific date without confirmation
/// kasl adjust --date 2025-01-15 --mode end --minutes 15 --force
/// ```
///
/// # Error Scenarios
///
/// - No workday found for the specified date
/// - Invalid date format
/// - Adjustment would result in negative work time
/// - Database connection or update failures
/// - Invalid time formats for pause mode
pub async fn cmd(args: AdjustArgs) -> Result<()> {
    let date = parse_date(&args.date)?;
    let mut workdays_db = Workdays::new()?;

    // Validate that a workday exists for the specified date
    let workday = match workdays_db.fetch(date)? {
        Some(wd) => wd,
        None => {
            msg_error!(Message::WorkdayNotFoundForDate(date.format("%B %-d, %Y").to_string()));
            return Ok(());
        }
    };

    // Collect adjustment parameters (interactively if not provided)
    let mode = args.mode.unwrap_or_else(|| {
        let options = vec!["Start - Remove time from start", "End - Remove time from end", "Pause - Add a pause"];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::SelectAdjustmentMode.to_string())
            .items(&options)
            .default(0)
            .interact()
            .unwrap();

        match selection {
            0 => AdjustmentMode::Start,
            1 => AdjustmentMode::End,
            _ => AdjustmentMode::Pause,
        }
    });

    let minutes = args.minutes.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptAdjustmentMinutes.to_string())
            .validate_with(|input: &u64| -> Result<(), &str> {
                if *input > 0 && *input <= 480 {
                    // Maximum 8 hours
                    Ok(())
                } else {
                    Err("Minutes must be between 1 and 480")
                }
            })
            .interact_text()
            .unwrap()
    });

    // Calculate and display preview of proposed changes
    let preview = calculate_preview(&workday, &mode, minutes)?;
    display_preview(&workday, &preview, &mode, minutes)?;

    // Request confirmation unless --force flag is used
    if !args.force {
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::ConfirmTimeAdjustment.to_string())
            .default(false)
            .interact()?;

        if !confirmed {
            msg_info!(Message::OperationCancelled);
            return Ok(());
        }
    }

    // Apply the requested changes to the database
    apply_adjustment(&mut workdays_db, &workday, &mode, minutes, date)?;

    msg_success!(Message::TimeAdjustmentApplied);
    Ok(())
}

/// Preview data structure for adjustment calculations.
///
/// This structure holds the calculated results of an adjustment operation
/// before it's applied, allowing for preview display and validation.
#[derive(Debug)]
struct AdjustmentPreview {
    /// New start time after adjustment
    new_start: NaiveDateTime,
    /// New end time after adjustment (None if unchanged)
    new_end: Option<NaiveDateTime>,
    /// Calculated work duration after adjustment
    new_duration: Duration,
    /// Pause to be added (start and end times) for pause mode
    pause_to_add: Option<(NaiveDateTime, NaiveDateTime)>,
}

/// Calculates the preview of an adjustment operation.
///
/// This function performs all the calculations for an adjustment without
/// actually modifying the database, allowing for validation and preview
/// display before committing changes.
///
/// # Arguments
///
/// * `workday` - The current workday record to be adjusted
/// * `mode` - The type of adjustment to perform
/// * `minutes` - The duration of the adjustment in minutes
///
/// # Returns
///
/// Returns an `AdjustmentPreview` with calculated new times and durations,
/// or an error if the adjustment would create an invalid state.
fn calculate_preview(workday: &crate::db::workdays::Workday, mode: &AdjustmentMode, minutes: u64) -> Result<AdjustmentPreview> {
    let duration_to_adjust = Duration::minutes(minutes as i64);
    let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
    let current_duration = end_time - workday.start;

    match mode {
        AdjustmentMode::Start => {
            let new_start = workday.start + duration_to_adjust;
            if new_start >= end_time {
                msg_bail_anyhow!(Message::InvalidAdjustmentTooMuchTime);
            }
            Ok(AdjustmentPreview {
                new_start,
                new_end: workday.end,
                new_duration: current_duration - duration_to_adjust,
                pause_to_add: None,
            })
        }
        AdjustmentMode::End => {
            let new_end = end_time - duration_to_adjust;
            if new_end <= workday.start {
                msg_bail_anyhow!(Message::InvalidAdjustmentTooMuchTime);
            }
            Ok(AdjustmentPreview {
                new_start: workday.start,
                new_end: Some(new_end),
                new_duration: current_duration - duration_to_adjust,
                pause_to_add: None,
            })
        }
        AdjustmentMode::Pause => {
            // For pause mode, prompt for the pause start time
            let pause_start_time = Input::<String>::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::PromptPauseStartTime.to_string())
                .default("12:00".to_string())
                .validate_with(|input: &String| -> Result<(), &str> {
                    NaiveTime::parse_from_str(input, "%H:%M")
                        .map(|_| ())
                        .map_err(|_| "Invalid time format. Use HH:MM")
                })
                .interact_text()?;

            let pause_time = NaiveTime::parse_from_str(&pause_start_time, "%H:%M")?;
            let pause_start = workday.start.date().and_time(pause_time);
            let pause_end = pause_start + duration_to_adjust;

            // Validate pause is within workday boundaries
            if pause_start < workday.start || pause_end > end_time {
                msg_bail_anyhow!(Message::InvalidPauseOutsideWorkday);
            }

            Ok(AdjustmentPreview {
                new_start: workday.start,
                new_end: workday.end,
                new_duration: current_duration - duration_to_adjust,
                pause_to_add: Some((pause_start, pause_end)),
            })
        }
    }
}

/// Displays a preview of the proposed adjustment changes.
///
/// This function shows a before/after comparison of the workday times,
/// helping users understand exactly what changes will be applied.
///
/// # Arguments
///
/// * `workday` - The current workday record
/// * `preview` - Calculated preview of changes
/// * `_mode` - The adjustment mode (currently unused)
/// * `minutes` - Duration being adjusted
fn display_preview(workday: &crate::db::workdays::Workday, preview: &AdjustmentPreview, _mode: &AdjustmentMode, minutes: u64) -> Result<()> {
    let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
    let current_duration = end_time - workday.start;

    msg_print!(Message::AdjustmentPreview, true);

    println!("Current workday:");
    println!("  Start: {}", workday.start.format("%H:%M"));
    println!("  End:   {}", end_time.format("%H:%M"));
    println!("  Total: {}", format_duration(&current_duration));

    println!("\nAfter adjustment:");
    println!("  Start: {}", preview.new_start.format("%H:%M"));
    println!("  End:   {}", preview.new_end.unwrap_or(end_time).format("%H:%M"));
    println!(
        "  Total: {} (-{})",
        format_duration(&preview.new_duration),
        format_duration(&Duration::minutes(minutes as i64))
    );

    if let Some((pause_start, pause_end)) = &preview.pause_to_add {
        println!("\nNew pause:");
        println!("  {} - {}", pause_start.format("%H:%M"), pause_end.format("%H:%M"));
    }

    Ok(())
}

/// Applies the calculated adjustment to the database.
///
/// This function performs the actual database updates based on the
/// adjustment mode and calculated values. It handles all three
/// adjustment types with appropriate database operations.
///
/// # Arguments
///
/// * `workdays_db` - Database connection for workday updates
/// * `workday` - The current workday record
/// * `mode` - Type of adjustment being applied
/// * `minutes` - Duration of adjustment in minutes
/// * `date` - Date of the workday being adjusted
fn apply_adjustment(workdays_db: &mut Workdays, workday: &crate::db::workdays::Workday, mode: &AdjustmentMode, minutes: u64, date: NaiveDate) -> Result<()> {
    let duration_to_adjust = Duration::minutes(minutes as i64);

    match mode {
        AdjustmentMode::Start => {
            let new_start = workday.start + duration_to_adjust;
            workdays_db.update_start(date, new_start)?;
        }
        AdjustmentMode::End => {
            let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
            let new_end = end_time - duration_to_adjust;
            workdays_db.update_end(date, Some(new_end))?;
        }
        AdjustmentMode::Pause => {
            // Re-prompt for pause start time (duplicates preview logic for consistency)
            let pause_start_time = Input::<String>::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::PromptPauseStartTime.to_string())
                .default("12:00".to_string())
                .interact_text()?;

            let pause_time = NaiveTime::parse_from_str(&pause_start_time, "%H:%M")?;
            let pause_start = workday.start.date().and_time(pause_time);
            let pause_end = pause_start + duration_to_adjust;

            // Insert the pause record
            let pauses_db = Pauses::new()?;
            pauses_db.insert_start_with_time(pause_start)?;

            // Update the pause with end time and duration
            let pauses = pauses_db.get_daily_pauses(date, 0)?;
            if let Some(last_pause) = pauses.iter().rev().find(|p| p.start == pause_start) {
                let conn_guard = pauses_db.conn.lock();
                conn_guard.execute(
                    "UPDATE pauses SET end = ?1, duration = ?2 WHERE id = ?3",
                    rusqlite::params![
                        pause_end.format("%Y-%m-%d %H:%M:%S").to_string(),
                        duration_to_adjust.num_seconds(),
                        last_pause.id
                    ],
                )?;
            }
        }
    }

    Ok(())
}

/// Parses a date string supporting both 'today' and ISO format.
///
/// This helper function provides consistent date parsing across the
/// adjust command, supporting user-friendly input formats.
///
/// # Arguments
///
/// * `date_str` - Date string to parse ('today' or 'YYYY-MM-DD')
///
/// # Returns
///
/// Returns the parsed date or an error for invalid formats.
fn parse_date(date_str: &str) -> Result<NaiveDate> {
    if date_str.to_lowercase() == "today" {
        Ok(Local::now().date_naive())
    } else {
        Ok(NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?)
    }
}
