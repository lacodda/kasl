//! `kasl demo`: eight invented weeks to try kasl on.
//!
//! ```bash
//! kasl demo                 # build the demo and say what to try
//! kasl demo report          # any command, on the demo's records
//! kasl demo sum
//! kasl demo --reset         # build it anew
//! ```
//!
//! The command itself only builds the demo and introduces it. A command
//! written after the word - `kasl demo report` - never reaches this module:
//! the word is taken off the command line before it is parsed (see
//! [`crate::libs::demo`]) and the command runs as it always does, on the
//! demo's data directory.

use crate::libs::demo::{self, Demo, Ensured};
use crate::libs::formatter::format_span;
use crate::libs::status::Status;
use crate::msg_print;
use anyhow::{Result, bail};
use chrono::Local;
use clap::Args;

/// Command-line arguments for the demo command.
#[derive(Debug, Args)]
pub struct DemoArgs {
    /// Build the demo anew, dropping whatever was added to it
    #[arg(long)]
    reset: bool,

    /// A kasl command to run on the demo: `kasl demo report`, `kasl demo task list`
    #[arg(value_name = "COMMAND", trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

/// Builds the demo when there is none (or with `--reset`), and says what to try on it.
pub fn cmd(args: DemoArgs) -> Result<()> {
    // Reached only when a flag stands between the word and the command;
    // `kasl demo report` is taken apart before clap sees it.
    if !args.command.is_empty() {
        bail!(
            "give the command right after the word: `kasl demo {}`; `kasl demo --reset` builds the demo anew",
            args.command.join(" ")
        );
    }

    // This command is the one that runs in the demo without a command after
    // the word, so it enters by itself.
    demo::enter();
    let now = Local::now().naive_local();
    let built = if args.reset {
        demo::build(now)?;
        Ensured::Rebuilt
    } else {
        demo::ensure(now)?
    };
    let summary = Demo::generate(now).summary();

    let opening = match built {
        Ensured::Kept => "The demo is ready",
        Ensured::Built => "The demo is built",
        Ensured::Rebuilt => "The demo is built anew",
    };
    msg_print!(format!(
        "{opening}: {} working days from {} to {}, {} tasks and an inbox of {} issues - all invented, and kept apart from your own records.",
        summary.days,
        summary.from.format("%B %-d"),
        summary.to.format("%B %-d"),
        summary.tasks,
        summary.issues
    ));

    // Read through the command that prints it, so the introduction cannot
    // promise a day the demo does not show.
    let status = Status::read()?;
    if status.workday.is_some() {
        msg_print!(format!("Today in it: {} worked so far.", format_span(&status.work)));
    }

    msg_print!("");
    msg_print!("Put `demo` before any command to run it there:");
    msg_print!("");
    for (command, what) in [
        ("kasl demo report", "today: the hours between pauses, and the tasks"),
        ("kasl demo report --last", "yesterday, a finished day"),
        ("kasl demo sum", "the month, day by day"),
        ("kasl demo standup --week", "the week as markdown, for a standup"),
        ("kasl demo task list", "today's tasks"),
        ("kasl demo inbox", "the issues waiting, as a tracker would fill it"),
        ("kasl demo status", "one line for a shell prompt"),
    ] {
        msg_print!(format!("  {command:<26}{what}"));
    }
    msg_print!("");
    msg_print!("Anything you add there stays there, until the next day or `kasl demo --reset` builds the demo anew.");
    msg_print!("Your own day starts with `kasl watch`.");
    Ok(())
}
