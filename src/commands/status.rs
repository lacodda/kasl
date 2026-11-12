//! One line for a shell prompt or a status bar: today's work, the watcher, the inbox.
//!
//! ## Usage
//!
//! ```bash
//! kasl status           # 6h 40m | paused 12m | inbox 9 (3 new)
//! kasl status --json    # the same, field by field, for a bar that draws its own
//! ```
//!
//! Nothing to say prints nothing, so a prompt segment built on it simply
//! disappears; only a person at a terminal is told why the line is empty.
//! The line reads local files only and never checks for updates: a prompt
//! runs it every time it draws.

use crate::{
    libs::{
        messages::Message,
        output::{self, Format},
        status::Status,
    },
    msg_info,
};
use anyhow::Result;
use clap::Args;
use std::io::IsTerminal;

/// Command-line arguments for the status line.
#[derive(Debug, Args)]
pub struct StatusArgs {
    #[command(flatten)]
    format: Format,
}

/// Prints today's status line, or its document with `--json`.
pub fn cmd(args: StatusArgs) -> Result<()> {
    let status = Status::read()?;
    if args.format.json {
        return output::print_json(&status.to_json());
    }

    let line = status.line();
    if !line.is_empty() {
        // Data, not a message: printed as it is even in a debug run, which
        // would send a message to the log instead.
        println!("{line}");
    } else if std::io::stdout().is_terminal() {
        // A prompt reads the empty line as "nothing to show"; a person who
        // typed the command reads it as a hang or a crash.
        msg_info!(Message::StatusNothingToSay);
    }
    Ok(())
}
