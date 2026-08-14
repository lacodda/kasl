//! Pause viewing and manual pause recording.
//!
//! Shows the absences detected by the activity monitor and lets the user record
//! the ones it missed - a walk with the laptop closed, a meeting away from the
//! desk - by stating when they happened.
//!
//! ## Usage
//!
//! ```bash
//! # Show today's pauses
//! kasl pauses list
//!
//! # Show pauses for a specific date
//! kasl pauses list --date 2025-01-15
//!
//! # Record a 45-minute absence starting at 13:00
//! kasl pauses add --start 13:00 --minutes 45
//!
//! # Record a short absence that must survive the duration filter
//! kasl pauses add --start 16:20 --minutes 10 --keep
//!
//! # Remove a pause by id
//! kasl pauses remove 42
//! ```

use crate::db::pauses::Pauses;
use crate::db::workdays::Workdays;
use crate::libs::config::Config;
use crate::libs::formatter::parse_date;
use crate::libs::messages::Message;
use crate::libs::pick;
use crate::libs::view::View;
use crate::{msg_error, msg_print, msg_success};
use anyhow::{Result, bail};
use chrono::{Duration, NaiveTime, TimeDelta};
use clap::{Args, Subcommand};
use dialoguer::{Confirm, theme::ColorfulTheme};
use std::io::IsTerminal;

/// Command-line arguments for the pauses command.
#[derive(Debug, Args)]
pub struct PausesArgs {
    #[command(subcommand)]
    command: Option<PausesCommand>,

    // Kept at the top level so that the bare `kasl pauses --date X` form keeps
    // working as a shorthand for `kasl pauses list --date X`.
    /// Date to fetch pauses for (YYYY-MM-DD or 'today')
    #[arg(long, short, default_value = "today")]
    date: String,

    /// Minimum pause duration filter in minutes
    #[arg(long, short, help = "Minimum pause duration in minutes")]
    min_duration: Option<u64>,
}

/// Subcommands for viewing and editing pause records.
#[derive(Debug, Subcommand)]
enum PausesCommand {
    /// Record an absence the activity monitor did not detect
    #[command(about = "Record a pause that the monitor missed")]
    Add(AddArgs),

    /// List pauses for a date
    #[command(about = "List pauses for a given date")]
    List(ListArgs),

    /// Remove a pause by id
    #[command(about = "Remove a pause record")]
    Remove(RemoveArgs),
}

/// Arguments for recording a manual pause.
#[derive(Debug, Args)]
struct AddArgs {
    /// When the absence began (HH:MM, on the given date)
    #[arg(long, short, value_name = "HH:MM", help = "Start time of the absence")]
    start: String,

    /// How long the absence lasted, in minutes
    #[arg(long, short, value_name = "N", help = "Duration in minutes")]
    minutes: u64,

    /// Date the absence belongs to
    #[arg(long, short, default_value = "today", help = "Date of the absence (YYYY-MM-DD or 'today')")]
    date: String,

    /// Keep this pause regardless of the duration threshold
    ///
    /// Protected pauses are never dropped by the minimum-duration filter and are
    /// never merged into an adjacent pause, so a deliberately short entry stays
    /// exactly as recorded.
    #[arg(long, help = "Exempt this pause from filtering and merging")]
    keep: bool,

    /// Optional note describing the absence
    #[arg(long, short, help = "Note describing the absence")]
    reason: Option<String>,
}

/// Arguments for listing pauses.
#[derive(Debug, Args)]
struct ListArgs {
    /// Date to fetch pauses for
    #[arg(long, short, default_value = "today", help = "Date to fetch pauses for (YYYY-MM-DD or 'today')")]
    date: String,

    /// Minimum pause duration filter in minutes
    #[arg(long, short, help = "Minimum pause duration in minutes")]
    min_duration: Option<u64>,
}

/// Arguments for removing a pause.
#[derive(Debug, Args)]
struct RemoveArgs {
    /// Identifier of the pause to remove
    #[arg(value_name = "ID", help = "Id of the pause to remove; omit to pick from the day")]
    id: Option<i32>,

    /// Date to pick a pause from, when no id was given
    #[arg(long, short, default_value = "today", help = "Date to pick from (YYYY-MM-DD or 'today')")]
    date: String,

    /// Remove without asking for confirmation
    #[arg(long, short = 'y', help = "Do not ask for confirmation")]
    yes: bool,
}

/// Executes the pauses command.
pub async fn cmd(args: PausesArgs) -> Result<()> {
    match args.command {
        Some(PausesCommand::Add(add_args)) => add(add_args),
        Some(PausesCommand::Remove(remove_args)) => remove(remove_args),
        Some(PausesCommand::List(list_args)) => list(&list_args.date, list_args.min_duration),
        // Bare `kasl pauses` keeps showing the day, as it always has.
        None => list(&args.date, args.min_duration),
    }
}

/// Displays pauses for the given date.
fn list(date_str: &str, min_duration_override: Option<u64>) -> Result<()> {
    let date = parse_date(date_str)?;

    // Load configuration to get default minimum pause duration
    let config = Config::read()?;
    let min_duration = min_duration_override.unwrap_or(config.monitor.unwrap_or_default().min_pause_duration);

    // Fetch pause records; when a workday exists, keep only in-bounds pauses.
    let pauses_db = Pauses::new()?.set_min_duration(min_duration);
    let pauses = match Workdays::new()?.fetch(date)? {
        Some(workday) => pauses_db.get_workday_pauses(&workday)?,
        None => pauses_db.get_daily_pauses(date)?,
    };

    // Calculate total pause time for summary statistics
    let total_pause_time = pauses.iter().filter_map(|p| p.duration).fold(Duration::zero(), |acc, d| acc + d);

    // Display formatted results with date header
    msg_print!(Message::PausesTitle(date.format("%B %-d, %Y").to_string()), true);
    View::pauses(&pauses, total_pause_time)?;

    Ok(())
}

/// Records a manual pause with the exact bounds the user stated.
///
/// No placement is inferred: the user says when the absence began and how long
/// it lasted. An entry that would overlap an already recorded pause is rejected,
/// so the day never holds two contradictory accounts of the same minutes.
fn add(args: AddArgs) -> Result<()> {
    if args.minutes == 0 {
        bail!("duration must be at least 1 minute");
    }

    let date = parse_date(&args.date)?;
    let time = NaiveTime::parse_from_str(&args.start, "%H:%M").map_err(|_| anyhow::anyhow!("invalid start time '{}' - expected HH:MM", args.start))?;
    let start = date.and_time(time);
    let duration = TimeDelta::minutes(args.minutes as i64);
    let end = start + duration;

    let pauses = Pauses::new()?;

    // Reject an entry that collides with a pause already on record.
    if let Some(existing) = pauses.find_overlapping(start, end)? {
        msg_error!(Message::ManualPauseOverlaps {
            start_time: existing.start.format("%H:%M").to_string(),
            end_time: existing.end.map(|e| e.format("%H:%M").to_string()).unwrap_or_else(|| "…".to_string()),
        });
        return Ok(());
    }

    pauses.insert_manual(start, duration, args.keep, args.reason.as_deref())?;

    msg_success!(Message::ManualPauseCreated {
        start_time: start.format("%H:%M").to_string(),
        end_time: end.format("%H:%M").to_string(),
        duration_minutes: args.minutes,
    });

    Ok(())
}

/// Removes a pause record after confirmation.
fn remove(args: RemoveArgs) -> Result<()> {
    let pauses = Pauses::new()?;

    // With no id there is nothing to confirm yet: the picker shows the day and
    // the choice made there is the confirmation.
    let id = match args.id {
        Some(id) => id,
        None => {
            let date = parse_date(&args.date)?;
            let listed = pauses.get_daily_pauses(date)?;
            return remove_picked(&pauses, pick::pause(&listed, "Remove which pause?")?);
        }
    };

    if !args.yes {
        // Never block on a prompt when there is no one to answer it.
        if !std::io::stdin().is_terminal() {
            bail!("refusing to remove pause {} without --yes outside an interactive terminal", id);
        }

        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Remove pause {}?", id))
            .default(false)
            .interact()?;

        if !confirmed {
            return Ok(());
        }
    }

    remove_picked(&pauses, id)
}

/// Deletes the pause with `id` and reports what happened.
fn remove_picked(pauses: &Pauses, id: i32) -> Result<()> {
    if pauses.delete_many(&[id])? == 0 {
        msg_error!(Message::ManualPauseNotFound(id));
    } else {
        msg_success!(Message::ManualPauseRemoved(id));
    }

    Ok(())
}
