//! Manual break management command for productivity optimization.
//!
//! Provides functionality to add manual break periods to improve productivity
//! calculations and help users reach minimum productivity thresholds for report submission.

use crate::{
    db::{breaks::Breaks, pauses::Pauses, workdays::Workdays},
    libs::{config::Config, formatter::format_duration, messages::Message, pause::Pause, report},
    msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Input, Select};

/// Command-line arguments for the breaks command.
///
/// Supports both automatic break placement and interactive selection
/// of break timing and placement options.
#[derive(Debug, Args)]
pub struct BreaksArgs {
    /// Minutes duration for the break (automatic placement)
    ///
    /// When specified, the system will automatically find the optimal
    /// placement for a break of this duration. If not specified, enters
    /// interactive mode for manual break configuration.
    #[arg(long, short)]
    minutes: Option<u64>,

    /// Force creation even if productivity validation fails
    ///
    /// Bypasses normal productivity threshold checks and creates the break
    /// regardless of current productivity levels. Use with caution.
    #[arg(long)]
    force: bool,
}

/// Represents a break placement option with timing details.
///
/// Each option shows the user when and where a break would be placed,
/// providing context for informed decision making.
#[derive(Debug, Clone)]
pub struct BreakOption {
    /// Start time of the proposed break
    pub start: NaiveDateTime,
    /// End time of the proposed break
    pub end: NaiveDateTime,
    /// Duration of the break
    pub duration: Duration,
    /// Human-readable description of the placement
    pub description: String,
}

/// Main entry point for the breaks command.
///
/// Routes between automatic placement mode and interactive configuration
/// based on command line arguments provided by the user.
///
/// # Arguments
///
/// * `args` - Parsed command line arguments specifying break options
///
/// # Returns
///
/// Returns `Ok(())` on successful break creation, or an error if
/// validation fails or database operations encounter issues.
pub async fn cmd(args: BreaksArgs) -> Result<()> {
    let today = Local::now().date_naive();
    
    // Validate that we can only create breaks for today
    let config = Config::read()?;
    let productivity_config = config.productivity.unwrap_or_default();
    
    if let Some(minutes) = args.minutes {
        handle_automatic_break_placement(today, minutes, &productivity_config, args.force).await
    } else {
        handle_interactive_break_creation(today, &productivity_config, args.force).await
    }
}

/// Handles automatic break placement with specified duration.
///
/// Finds the optimal placement for a break of the given duration and
/// creates it without user interaction. Includes productivity validation
/// unless forced.
async fn handle_automatic_break_placement(
    date: NaiveDate,
    minutes: u64,
    productivity_config: &crate::libs::config::ProductivityConfig,
    _force: bool,
) -> Result<()> {
    // Validate break duration
    if minutes < productivity_config.min_break_duration {
        msg_error!(Message::BreakDurationPrompt {
            min_duration: productivity_config.min_break_duration,
            max_duration: productivity_config.max_break_duration,
        });
        return Ok(());
    }

    if minutes > productivity_config.max_break_duration {
        msg_error!(Message::BreakDurationPrompt {
            min_duration: productivity_config.min_break_duration,
            max_duration: productivity_config.max_break_duration,
        });
        return Ok(());
    }

    // Get workday and pauses data
    let workday = match Workdays::new()?.fetch(date)? {
        Some(wd) => wd,
        None => {
            msg_error!("No workday found for today");
            return Ok(());
        }
    };

    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();
    let pauses = Pauses::new()?.get_daily_pauses(date, monitor_config.min_pause_duration)?;

    // Find optimal break placement
    let break_options = find_break_placement_options(&workday, &pauses, minutes, monitor_config.min_work_interval)?;
    
    if break_options.is_empty() {
        msg_error!(Message::NoValidBreakPlacement);
        return Ok(());
    }

    // Use the first (optimal) option
    let break_option = &break_options[0];
    
    // Create the break record
    let break_record = crate::db::breaks::Break {
        id: None,
        date,
        start: break_option.start,
        end: break_option.end,
        duration: break_option.duration,
        reason: None,
        created_at: None,
    };

    let breaks_db = Breaks::new()?;
    breaks_db.insert(&break_record)?;

    msg_success!(Message::BreakCreated {
        start_time: break_option.start.format("%H:%M").to_string(),
        end_time: break_option.end.format("%H:%M").to_string(),
        duration_minutes: minutes,
    });

    // Recalculate and show productivity
    show_updated_productivity(date).await?;
    
    Ok(())
}

/// Handles interactive break creation with user selection.
///
/// Prompts user for break duration and presents placement options
/// for selection. Provides full control over break timing and placement.
async fn handle_interactive_break_creation(
    date: NaiveDate,
    productivity_config: &crate::libs::config::ProductivityConfig,
    _force: bool,
) -> Result<()> {
    msg_print!(Message::BreakInteractivePrompt);

    // Prompt for break duration
    let theme = ColorfulTheme::default();
    let duration_input: String = Input::with_theme(&theme)
        .with_prompt(&format!("Enter break duration ({}-{} minutes)", productivity_config.min_break_duration, productivity_config.max_break_duration))
        .interact_text()?;

    let minutes: u64 = match duration_input.parse() {
        Ok(m) if m >= productivity_config.min_break_duration && m <= productivity_config.max_break_duration => m,
        _ => {
            msg_error!(Message::BreakDurationPrompt {
                min_duration: productivity_config.min_break_duration,
                max_duration: productivity_config.max_break_duration,
            });
            return Ok(());
        }
    };

    // Get workday and pauses data
    let workday = match Workdays::new()?.fetch(date)? {
        Some(wd) => wd,
        None => {
            msg_error!("No workday found for today");
            return Ok(());
        }
    };

    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();
    let pauses = Pauses::new()?.get_daily_pauses(date, monitor_config.min_pause_duration)?;

    // Find break placement options
    let break_options = find_break_placement_options(&workday, &pauses, minutes, monitor_config.min_work_interval)?;
    
    if break_options.is_empty() {
        msg_error!(Message::NoValidBreakPlacement);
        return Ok(());
    }

    // Present options to user
    msg_print!(Message::BreakPlacementOptions);
    let option_labels: Vec<String> = break_options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            format!(
                "{}. {} - {} ({} min) - {}",
                i + 1,
                opt.start.format("%H:%M"),
                opt.end.format("%H:%M"),
                opt.duration.num_minutes(),
                opt.description
            )
        })
        .collect();

    let selection = Select::with_theme(&theme)
        .with_prompt("Select break placement")
        .items(&option_labels)
        .default(0)
        .interact()?;

    let chosen_option = &break_options[selection];

    // Create the break record
    let break_record = crate::db::breaks::Break {
        id: None,
        date,
        start: chosen_option.start,
        end: chosen_option.end,
        duration: chosen_option.duration,
        reason: None,
        created_at: None,
    };

    let breaks_db = Breaks::new()?;
    breaks_db.insert(&break_record)?;

    msg_success!(Message::BreakCreated {
        start_time: chosen_option.start.format("%H:%M").to_string(),
        end_time: chosen_option.end.format("%H:%M").to_string(),
        duration_minutes: minutes,
    });

    // Recalculate and show productivity
    show_updated_productivity(date).await?;

    Ok(())
}

/// Finds optimal placement options for a break of the given duration.
///
/// Analyzes the workday and existing pauses to suggest the best times
/// to place a break, avoiding conflicts and maintaining minimum work intervals.
fn find_break_placement_options(
    workday: &crate::db::workdays::Workday,
    pauses: &[Pause],
    duration_minutes: u64,
    min_work_interval: u64,
) -> Result<Vec<BreakOption>> {
    let mut options = Vec::new();
    let current_time = Local::now().naive_local();
    let workday_end = workday.end.unwrap_or(current_time);
    
    // Calculate work intervals
    let intervals = report::calculate_work_intervals(workday, pauses);
    
    // Find gaps between pauses that can accommodate the break
    let break_duration = Duration::minutes(duration_minutes as i64);
    
    if intervals.is_empty() {
        return Ok(options);
    }

    // Strategy 1: Place break in the middle of the longest interval
    let longest_interval = intervals
        .iter()
        .max_by_key(|interval| interval.duration.num_minutes());
    
    if let Some(interval) = longest_interval {
        // Check if the interval is long enough to accommodate the break plus minimum work time
        let required_time = break_duration + Duration::minutes(min_work_interval as i64 * 2);
        if interval.duration >= required_time && interval.end <= current_time {
            let interval_mid = interval.start + (interval.duration / 2);
            let break_start = interval_mid - (break_duration / 2);
            let break_end = break_start + break_duration;
            
            options.push(BreakOption {
                start: break_start,
                end: break_end,
                duration: break_duration,
                description: "Middle of longest work period".to_string(),
            });
        }
    }

    // Strategy 2: Place break after existing pauses (if there's room)
    for (i, pause) in pauses.iter().enumerate() {
        if let Some(pause_end) = pause.end {
            // Find the next pause or end of workday
            let next_pause_start = pauses
                .get(i + 1)
                .map(|p| p.start)
                .unwrap_or(workday_end.min(current_time));
            
            let available_time = next_pause_start - pause_end;
            let required_time = break_duration + Duration::minutes(min_work_interval as i64);
            
            if available_time >= required_time && pause_end + break_duration <= current_time {
                options.push(BreakOption {
                    start: pause_end,
                    end: pause_end + break_duration,
                    duration: break_duration,
                    description: format!("After {} pause", format_duration(&pause.duration.unwrap_or_default())),
                });
            }
        }
    }

    // Strategy 3: Place break before existing pauses (if there's room)
    for pause in pauses.iter() {
        let work_start = workday.start;
        let available_time = pause.start - work_start;
        let required_time = break_duration + Duration::minutes(min_work_interval as i64);
        
        if available_time >= required_time {
            let break_end = pause.start - Duration::minutes(min_work_interval as i64);
            let break_start = break_end - break_duration;
            
            if break_start >= work_start && break_end <= current_time {
                options.push(BreakOption {
                    start: break_start,
                    end: break_end,
                    duration: break_duration,
                    description: format!("Before {} pause", format_duration(&pause.duration.unwrap_or_default())),
                });
            }
        }
    }

    // Remove duplicates and sort by start time
    options.sort_by_key(|opt| opt.start);
    options.dedup_by(|a, b| {
        (a.start - b.start).num_minutes().abs() < 5 // Consider times within 5 minutes as duplicates
    });

    // Limit to top 3 options
    options.truncate(3);
    
    Ok(options)
}

/// Shows updated productivity after break creation.
///
/// Recalculates productivity including the new break and displays
/// the improved productivity percentage to the user.
async fn show_updated_productivity(date: NaiveDate) -> Result<()> {
    // Get all data for productivity calculation
    let workday = Workdays::new()?.fetch(date)?.expect("Workday should exist");
    let config = Config::read()?;
    let _monitor_config = config.monitor.unwrap_or_default();
    
    let pauses = Pauses::new()?.get_daily_pauses(date, 0)?; // All pauses for calculation
    let breaks = Breaks::new()?.get_daily_breaks(date)?;
    
    // Calculate new productivity (this would be implemented in report.rs)
    let productivity = calculate_productivity_with_breaks(&workday, &pauses, &breaks)?;
    
    msg_info!(Message::ProductivityRecalculated(productivity));
    Ok(())
}

/// Calculates productivity including manual breaks.
///
/// Temporary implementation - this should be moved to report.rs module
/// as part of the productivity calculation update.
fn calculate_productivity_with_breaks(
    workday: &crate::db::workdays::Workday,
    pauses: &[Pause],
    breaks: &[crate::db::breaks::Break],
) -> Result<f64> {
    let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
    let gross_duration = end_time - workday.start;
    
    // Calculate total pause time
    let pause_duration: Duration = pauses
        .iter()
        .filter_map(|p| p.duration)
        .sum();
    
    // Calculate total break time
    let break_duration: Duration = breaks
        .iter()
        .map(|b| b.duration)
        .sum();
    
    let total_non_work_time = pause_duration + break_duration;
    let net_work_time = gross_duration - total_non_work_time;
    
    let productivity = if gross_duration.num_seconds() > 0 {
        (net_work_time.num_seconds() as f64 / gross_duration.num_seconds() as f64) * 100.0
    } else {
        0.0
    };
    
    Ok(productivity.max(0.0).min(100.0))
}