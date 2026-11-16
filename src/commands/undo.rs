//! Putting back what a remove command took away.
//!
//! `kasl task remove`, `kasl pauses remove`, `kasl tag remove` and
//! `kasl template remove` move what they delete to the trash; `kasl undo`
//! puts the latest removal back, whole, and run again goes one removal
//! further.
//!
//! ## Usage
//!
//! ```bash
//! # Put back what the last remove command took away
//! kasl undo
//!
//! # See what can be put back, the latest removal first
//! kasl undo --list
//! ```

use crate::db::server_outbox::ServerOutbox;
use crate::db::trash::{Kind, RETENTION_DAYS, Removal, Trash};
use crate::db::workdays::Workdays;
use crate::libs::config::Config;
use crate::libs::json;
use crate::libs::messages::Message;
use crate::libs::output::{self, Format};
use crate::libs::pulse::format_ago;
use crate::{msg_info, msg_print, msg_success};
use anyhow::Result;
use chrono::{NaiveDate, Utc};
use clap::Args;

/// Why a day put back by `kasl undo` is in the outbox, as `kasl server queue` shows it.
const QUEUED_BECAUSE: &str = "put back by kasl undo";

/// Command-line arguments for the undo command.
#[derive(Debug, Args)]
pub struct UndoArgs {
    /// Show what can be put back, the latest removal first, and change nothing
    #[arg(long)]
    list: bool,

    #[command(flatten)]
    format: Format,
}

/// Executes the undo command.
pub fn cmd(args: UndoArgs) -> Result<()> {
    if args.list {
        return list(args.format.json);
    }

    let Some(removal) = Trash::new()?.restore_last()? else {
        if args.format.json {
            return output::print_json(&json::Undo {
                restored: None,
                queued: vec![],
            });
        }
        msg_print!(Message::UndoNothing(RETENTION_DAYS));
        return Ok(());
    };

    let queued = queue_for_the_server(&removal)?;

    if args.format.json {
        return output::print_json(&json::Undo {
            restored: Some(json::Removal::from(&removal)),
            queued,
        });
    }

    msg_success!(Message::UndoRestored {
        what: what(&removal),
        command: removal.kind.command().to_string(),
        ago: ago(&removal),
    });
    print_items(&removal);
    for date in queued {
        msg_info!(Message::KaslServerDayQueued(date.to_string()));
    }

    Ok(())
}

/// Lists the removals the trash still holds.
fn list(json: bool) -> Result<()> {
    let removals = Trash::new()?.list()?;
    if json {
        return output::print_json(&json::Trash {
            kept_days: RETENTION_DAYS,
            removals: removals.iter().map(json::Removal::from).collect(),
        });
    }

    if removals.is_empty() {
        msg_print!(Message::UndoNothing(RETENTION_DAYS));
        return Ok(());
    }

    for removal in &removals {
        msg_print!(Message::UndoListEntry {
            what: what(removal),
            command: removal.kind.command().to_string(),
            ago: ago(removal),
        });
        print_items(removal);
    }

    Ok(())
}

/// Queues the days a restored task or pause belongs to, when this machine
/// reports to a kasl-server.
///
/// An upload says "these are all the tasks of the day", so a day pushed
/// between the removal and the undo left the server without the task; only
/// sending the day again brings it back there. The outbox holds a date, and
/// the day is rebuilt when it goes - whether or not it went in between is not
/// worth finding out. A date with no workday has nothing to send.
fn queue_for_the_server(removal: &Removal) -> Result<Vec<NaiveDate>> {
    if !matches!(removal.kind, Kind::Task | Kind::Pause) || Config::read().unwrap_or_default().kasl_server.is_none() {
        return Ok(vec![]);
    }

    let mut dates: Vec<NaiveDate> = removal.items.iter().filter_map(|item| item.date).collect();
    dates.sort_unstable();
    dates.dedup();

    let mut workdays = Workdays::new()?;
    let mut outbox = ServerOutbox::new()?;
    let mut queued = Vec::new();
    for date in dates {
        if workdays.fetch(date)?.is_some() {
            outbox.enqueue(date, QUEUED_BECAUSE)?;
            queued.push(date);
        }
    }
    Ok(queued)
}

/// One line per row of a removal, under its heading.
fn print_items(removal: &Removal) {
    for item in &removal.items {
        msg_print!(Message::UndoItem {
            id: item.id,
            label: item.label.clone(),
            date: item.date.map(|date| date.to_string()),
            renumbered_from: item.renumbered_from,
        });
    }
}

/// "2 tasks", "1 pause".
fn what(removal: &Removal) -> String {
    let count = removal.items.len();
    format!("{} {}{}", count, removal.kind.name(), if count == 1 { "" } else { "s" })
}

/// How long ago the removal happened, in words.
fn ago(removal: &Removal) -> String {
    format_ago((Utc::now() - removal.removed_at).num_seconds().max(0))
}
