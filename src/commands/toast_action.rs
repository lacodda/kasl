//! The courier a toast button launches.
//!
//! `kasl toast-action take PROJ-1` does one thing: it appends the ask to the
//! mailbox and exits. The daemon, which owns the database, performs it.
//!
//! This is not a way to triage from the shell - `kasl inbox take` already is,
//! and it reports what happened. The courier prints nothing, because nobody
//! is watching: it runs from a shortcut, minimized and unactivated, in the
//! moment after a button press.
//!
//! When no daemon is running, the ask would sit in the mailbox unread. That
//! is worse than doing nothing, because the button appeared to work, so the
//! courier performs the action itself in that case and says so where it can
//! be found: the log.

use crate::libs::toast_action::{Mailbox, ToastAction, ToastRequest};
use crate::libs::{daemon, messages::Message};
use crate::msg_error_anyhow;
use anyhow::Result;
use clap::Args;
use tracing::{debug, warn};

/// Arguments of the courier: which decision, on which issue.
#[derive(Debug, Args)]
pub struct ToastActionArgs {
    /// take, snooze or dismiss
    #[arg(value_name = "ACTION")]
    action: String,

    /// Issue key, e.g. PROJ-123
    #[arg(value_name = "KEY")]
    key: String,
}

/// Posts one toast decision, or performs it when no daemon is listening.
pub fn cmd(args: ToastActionArgs) -> Result<()> {
    let Some(action) = ToastAction::parse(&args.action) else {
        return Err(msg_error_anyhow!(Message::ToastActionUnknown(args.action.clone())));
    };
    let request = ToastRequest::new(action, &args.key);

    // A mailbox nobody drains is a button that lies. The daemon is the normal
    // case, and this is the fallback for a user who stopped the watcher and
    // still has a toast on screen.
    if !daemon::is_running() {
        warn!("No watcher is running; performing {} {} here instead", args.action, args.key);
        return crate::libs::toast_apply::apply(&request).map(|_| ());
    }

    Mailbox::open()?.post(&request)?;
    debug!("Posted {} {} to the toast mailbox", args.action, args.key);
    Ok(())
}
