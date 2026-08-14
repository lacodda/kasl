//! Jira inbox command: sync, list, pin, dismiss, open, and import issues.
//!
//! Manages the local `jira_inbox` table populated from assigned open Jira issues.

use crate::db::jira_inbox::JiraInbox;
use crate::db::tasks::Tasks;
use crate::libs::jira_inbox as inbox_lib;
use crate::libs::messages::Message;
use crate::libs::pick;
use crate::libs::task::Task;
use crate::libs::view::View;
use crate::{msg_error, msg_info, msg_print, msg_success};
use anyhow::{Result, bail};
use clap::{Args, Subcommand};

/// Command-line arguments for the inbox command.
#[derive(Debug, Args)]
pub struct InboxArgs {
    #[command(subcommand)]
    command: Option<InboxCommand>,

    // Kept at the top level so the bare `kasl inbox -n 5` form keeps working
    // as a shorthand for `kasl inbox list -n 5`.
    /// Show only the top N issues (already sorted by pin / score / priority)
    #[arg(long, short = 'n', value_name = "N")]
    limit: Option<usize>,

    /// Include issues gone from Jira (closed or reassigned)
    #[arg(long)]
    all: bool,
}

/// Available inbox operations.
#[derive(Debug, Subcommand)]
enum InboxCommand {
    /// Sync assigned open issues from Jira now
    #[command(about = "Sync inbox from Jira")]
    Sync,

    /// List active (non-dismissed) inbox issues
    #[command(about = "List active inbox issues")]
    List {
        /// Show only the top N issues
        #[arg(long, short = 'n', value_name = "N")]
        limit: Option<usize>,

        /// Include issues gone from Jira (closed or reassigned)
        #[arg(long)]
        all: bool,
    },

    /// Pin an issue so it stays on top
    #[command(about = "Pin an inbox issue")]
    Pin {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,
    },

    /// Unpin a previously pinned issue
    #[command(about = "Unpin an inbox issue")]
    Unpin {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,
    },

    /// Dismiss an issue, hiding it from the list
    #[command(about = "Dismiss an inbox issue")]
    Dismiss {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,
    },

    /// Open an issue in the browser
    #[command(about = "Open issue URL in browser")]
    Open {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,
    },

    /// Import an issue into local tasks
    #[command(about = "Import issue into tasks")]
    Take {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,
    },
}

/// Entry point for `kasl inbox`.
pub async fn cmd(args: InboxArgs) -> Result<()> {
    match args.command {
        Some(InboxCommand::Sync) => {
            let outcome = inbox_lib::sync_interactive(true).await?;
            if !outcome.skipped {
                msg_success!(Message::JiraInboxSynced {
                    fetched: outcome.fetched,
                    new_count: outcome.new_keys.len(),
                    changed: outcome.changed.len(),
                    gone: outcome.gone_keys.len(),
                });
            }
            Ok(())
        }
        Some(InboxCommand::List { limit, all }) => list_inbox(limit.or(args.limit), all || args.all),
        Some(InboxCommand::Pin { key }) => set_pinned(&resolve_key(key, "Pin which issue?", false)?, true),
        Some(InboxCommand::Unpin { key }) => set_pinned(&resolve_key(key, "Unpin which issue?", true)?, false),
        Some(InboxCommand::Dismiss { key }) => dismiss(&resolve_key(key, "Dismiss which issue?", false)?),
        Some(InboxCommand::Open { key }) => open_issue(&resolve_key(key, "Open which issue?", false)?),
        Some(InboxCommand::Take { key }) => take_issue(&resolve_key(key, "Take which issue?", false)?),
        // Bare `kasl inbox` shows the list, as it always has.
        None => list_inbox(args.limit, args.all),
    }
}

/// Returns the issue key to act on, opening a picker when none was given.
///
/// `pinned_only` narrows the list for `unpin`, where offering issues that are
/// not pinned would be offering work that does nothing.
fn resolve_key(key: Option<String>, prompt: &str, pinned_only: bool) -> Result<String> {
    if let Some(key) = key {
        return Ok(key);
    }
    let items = JiraInbox::new()?.list_active(false)?;
    if pinned_only {
        let pinned: Vec<_> = items.into_iter().filter(|i| i.pinned).collect();
        if pinned.is_empty() {
            bail!("no pinned issues - `kasl inbox pin KEY` pins one");
        }
        return pick::inbox_issue(&pinned, prompt);
    }
    pick::inbox_issue(&items, prompt)
}

fn list_inbox(limit: Option<usize>, include_gone: bool) -> Result<()> {
    let mut items = JiraInbox::new()?.list_active(include_gone)?;
    if items.is_empty() {
        msg_info!(Message::JiraInboxEmpty);
        return Ok(());
    }
    if let Some(n) = limit {
        items.truncate(n);
    }
    msg_print!(Message::JiraInboxListHeader, true);
    View::jira_inbox(&items)
}

fn set_pinned(key: &str, pinned: bool) -> Result<()> {
    let db = JiraInbox::new()?;
    if !db.set_pinned(key, pinned)? {
        msg_error!(Message::JiraInboxNotFound(key.to_string()));
        return Ok(());
    }
    if pinned {
        msg_success!(Message::JiraInboxPinned(key.to_string()));
    } else {
        msg_success!(Message::JiraInboxUnpinned(key.to_string()));
    }
    Ok(())
}

fn dismiss(key: &str) -> Result<()> {
    let db = JiraInbox::new()?;
    if !db.set_dismissed(key, true)? {
        msg_error!(Message::JiraInboxNotFound(key.to_string()));
        return Ok(());
    }
    msg_success!(Message::JiraInboxDismissed(key.to_string()));
    Ok(())
}

fn open_issue(key: &str) -> Result<()> {
    let db = JiraInbox::new()?;
    let Some(item) = db.get_by_key(key)? else {
        msg_error!(Message::JiraInboxNotFound(key.to_string()));
        return Ok(());
    };

    match inbox_lib::open_url(&item.url) {
        Ok(()) => msg_success!(Message::JiraInboxOpened(key.to_string())),
        Err(e) => msg_error!(Message::JiraInboxOpenFailed(e.to_string())),
    }
    Ok(())
}

fn take_issue(key: &str) -> Result<()> {
    let db = JiraInbox::new()?;
    let Some(item) = db.get_by_key(key)? else {
        msg_error!(Message::JiraInboxNotFound(key.to_string()));
        return Ok(());
    };

    let name = format!("{} {}", item.issue_key, item.summary);
    let task = Task::new(&name, "", Some(0));
    Tasks::new()?.insert(&task)?;
    let _ = db.set_dismissed(key, true)?;
    msg_success!(Message::JiraInboxTaken(key.to_string()));
    Ok(())
}
