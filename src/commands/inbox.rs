//! Jira inbox command: sync, list, pin, dismiss, open, and import issues.
//!
//! Manages the local `jira_inbox` table populated from assigned open Jira issues.

use crate::db::jira_inbox::JiraInbox;
use crate::db::tasks::Tasks;
use crate::libs::jira_inbox as inbox_lib;
use crate::libs::messages::Message;
use crate::libs::task::Task;
use crate::libs::view::View;
use crate::{msg_error, msg_info, msg_print, msg_success};
use anyhow::Result;
use clap::Args;

/// Command-line arguments for the inbox command.
#[derive(Debug, Args)]
pub struct InboxArgs {
    /// List active (non-dismissed) inbox issues
    #[arg(long, short = 'l', help = "List active inbox issues")]
    list: bool,

    /// Sync assigned open issues from Jira now
    #[arg(long, help = "Sync inbox from Jira")]
    sync: bool,

    /// Pin an issue by key (e.g. PROJ-123)
    #[arg(long, value_name = "KEY", help = "Pin an inbox issue")]
    pin: Option<String>,

    /// Unpin an issue by key
    #[arg(long, value_name = "KEY", help = "Unpin an inbox issue")]
    unpin: Option<String>,

    /// Dismiss an issue by key (hide from list)
    #[arg(long, value_name = "KEY", help = "Dismiss an inbox issue")]
    dismiss: Option<String>,

    /// Open an issue in the browser
    #[arg(long, value_name = "KEY", help = "Open issue URL in browser")]
    open: Option<String>,

    /// Import an issue into local tasks
    #[arg(long, value_name = "KEY", help = "Import issue into tasks")]
    take: Option<String>,
}

/// Entry point for `kasl inbox`.
pub async fn cmd(args: InboxArgs) -> Result<()> {
    let mut did_something = false;

    if args.sync {
        did_something = true;
        let outcome = inbox_lib::sync_interactive(true).await?;
        if !outcome.skipped {
            msg_success!(Message::JiraInboxSynced {
                fetched: outcome.fetched,
                new_count: outcome.new_keys.len(),
                updated: outcome.updated,
            });
        }
    }

    if let Some(key) = &args.pin {
        did_something = true;
        set_pinned(key, true)?;
    }
    if let Some(key) = &args.unpin {
        did_something = true;
        set_pinned(key, false)?;
    }
    if let Some(key) = &args.dismiss {
        did_something = true;
        dismiss(key)?;
    }
    if let Some(key) = &args.open {
        did_something = true;
        open_issue(key)?;
    }
    if let Some(key) = &args.take {
        did_something = true;
        take_issue(key)?;
    }

    // Default action (or explicit --list): show the table.
    if args.list || !did_something {
        list_inbox()?;
    }

    Ok(())
}

fn list_inbox() -> Result<()> {
    let items = JiraInbox::new()?.list_active()?;
    if items.is_empty() {
        msg_info!(Message::JiraInboxEmpty);
        return Ok(());
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
