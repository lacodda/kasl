//! What was done, as markdown: the day or the week, ready for a standup or a daily note.
//!
//! ## Usage
//!
//! ```bash
//! kasl standup                         # today
//! kasl standup --last                  # yesterday
//! kasl standup --week                  # this week, Monday to Sunday
//! kasl standup --week --last           # last week
//! kasl standup --append ~/notes/{date}.md   # into today's daily note
//! ```
//!
//! The hours are the ones `kasl report` counts and the lines are the
//! period's tasks. `--append` writes the summary into a markdown file between
//! two comment markers, so a second run replaces the first one's block.

use crate::{
    libs::{
        formatter::parse_date,
        messages::Message,
        standup::{self, Period, Placed, Standup},
    },
    msg_error_anyhow, msg_info, msg_success,
};
use anyhow::{Context, Result};
use chrono::{Duration, Local};
use clap::Args;

/// Command-line arguments for the standup summary.
#[derive(Debug, Args)]
pub struct StandupArgs {
    /// The week instead of the day: Monday to Sunday
    #[arg(long, short)]
    week: bool,

    /// The one before: yesterday, or with --week last week
    #[arg(long, short, conflicts_with = "date")]
    last: bool,

    /// The day to sum up, or with --week a day in the week (YYYY-MM-DD or 'today')
    #[arg(long, short)]
    date: Option<String>,

    /// Write the summary into a markdown file - a daily note - instead of printing it
    ///
    /// The file is created when it does not exist; its folder must. The
    /// summary goes between two comment markers at the end, and a second run
    /// for the same period replaces it there. `{date}` in the path is the day
    /// (a week's Monday), `{week}` the ISO week: `~/notes/{date}.md`.
    #[arg(long, value_name = "FILE")]
    append: Option<String>,
}

/// Prints the period's summary, or places it in a note with `--append`.
pub fn cmd(args: StandupArgs) -> Result<()> {
    let period = period(&args)?;
    let summary = Standup::read(period)?;
    if summary.is_empty() {
        msg_info!(Message::StandupNothingRecorded(period.label()));
        return Ok(());
    }

    let Some(template) = args.append else {
        // Data, not a message: printed as it is even in a debug run.
        println!("{}", summary.markdown());
        return Ok(());
    };

    let path = standup::note_path(&template, period);
    if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty())
        && !folder.is_dir()
    {
        return Err(msg_error_anyhow!(Message::StandupNoFolder(folder.display().to_string())));
    }
    let note = match std::fs::read_to_string(&path) {
        Ok(note) => note,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    let (text, placed) = standup::place(&note, &period.id(), &summary.markdown());
    std::fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    msg_success!(Message::StandupPlaced {
        period: period.label(),
        path: path.display().to_string(),
        replaced: placed == Placed::Replaced,
    });
    Ok(())
}

/// The period the flags name: the day or the week, today's or the one before, or the one a date falls in.
fn period(args: &StandupArgs) -> Result<Period> {
    let today = Local::now().date_naive();
    let date = match (&args.date, args.last, args.week) {
        (Some(date), _, _) => parse_date(date)?,
        (None, true, true) => today - Duration::days(7),
        (None, true, false) => today - Duration::days(1),
        (None, false, _) => today,
    };
    Ok(if args.week { Period::Week(date) } else { Period::Day(date) })
}
