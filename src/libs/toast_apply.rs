//! Performing a toast decision, in the one place both callers use.
//!
//! The effect of take / snooze / dismiss lives here and nowhere else, so a
//! button on a toast cannot drift from `kasl inbox take` in what it actually
//! does. The CLI wraps these with the terminal messages it prints; the daemon
//! wraps them with a toast saying what happened.
//!
//! Each function returns what it did rather than printing it, because the
//! daemon has no terminal to print to: a decision made from a toast has to
//! answer on a toast.

use crate::db::jira_inbox::JiraInbox;
use crate::db::tasks::Tasks;
use crate::libs::inbox_filter;
use crate::libs::task::{Task, TaskFilter};
use crate::libs::toast_action::{ToastAction, ToastRequest};
use anyhow::Result;
use chrono::{Local, NaiveDateTime};

/// What performing a decision came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// The issue became a task.
    Taken,
    /// The issue was already a task; the name it carries.
    AlreadyTaken(String),
    /// The issue is asleep until this moment.
    Snoozed(NaiveDateTime),
    /// The issue was hidden from the list.
    Dismissed,
    /// No such key in the inbox - it was synced away between the toast and
    /// the click.
    NotFound,
}

impl Applied {
    /// One line for a toast: what happened, from the user's side.
    pub fn summary(&self, key: &str) -> String {
        match self {
            Applied::Taken => format!("{key} is now a task"),
            Applied::AlreadyTaken(name) => format!("{key} was already taken as '{name}'"),
            Applied::Snoozed(until) => format!("{key} sleeps until {}", until.format("%b %-d %H:%M")),
            Applied::Dismissed => format!("{key} dismissed"),
            Applied::NotFound => format!("{key} is no longer in the inbox"),
        }
    }

    /// Whether the issue is settled and its buttons should stop existing.
    pub fn settled(&self) -> bool {
        !matches!(self, Applied::NotFound)
    }
}

/// Performs one request, whichever action it carries.
///
/// The snooze window comes from the config so a button and `inbox triage`
/// sleep for the same length; an unreadable window falls back to a day rather
/// than failing the click, because a button that silently does nothing is
/// worse than one that sleeps for the default.
pub fn apply(request: &ToastRequest) -> Result<Applied> {
    match request.action {
        ToastAction::Take => take(&request.issue_key),
        ToastAction::Snooze => {
            let window = snooze_window();
            snooze(&request.issue_key, window)
        }
        ToastAction::Dismiss => dismiss(&request.issue_key),
    }
}

/// How long a toast's Snooze button sleeps.
pub fn snooze_window() -> chrono::Duration {
    let configured = crate::libs::config::Config::read().ok().and_then(|c| c.jira_inbox).map(|i| i.toast_snooze_for);
    match configured.as_deref().map(inbox_filter::parse_window) {
        Some(Ok(window)) => window,
        _ => chrono::Duration::days(1),
    }
}

/// Turns an inbox issue into a task, as `kasl inbox take` does.
///
/// Taking the same issue twice must not fan out into duplicate tasks: from a
/// toast that is even likelier than from the shell, because a toast can be
/// clicked from the notification centre long after it appeared.
pub fn take(key: &str) -> Result<Applied> {
    let db = JiraInbox::new()?;
    let Some(item) = db.get_by_key(key)? else {
        return Ok(Applied::NotFound);
    };

    let mut tasks = Tasks::new()?;
    if let Some(task) = tasks.fetch(TaskFilter::ByJiraKey(key.to_string()))?.first() {
        // Repair the mark if only the task survived.
        if item.taken_at.is_none() {
            let _ = db.set_taken(key, true)?;
        }
        return Ok(Applied::AlreadyTaken(task.name.clone()));
    }

    let name = format!("{} {}", item.issue_key, item.summary);
    let task = Task::new(&name, "", Some(0)).from_jira(key);
    tasks.insert(&task)?;
    let _ = db.set_taken(key, true)?;
    Ok(Applied::Taken)
}

/// Puts an issue to sleep for `window`.
pub fn snooze(key: &str, window: chrono::Duration) -> Result<Applied> {
    let until = Local::now().naive_local() + window;
    let db = JiraInbox::new()?;
    if !db.set_snoozed(key, Some(until))? {
        return Ok(Applied::NotFound);
    }
    Ok(Applied::Snoozed(until))
}

/// Hides an issue from the list.
pub fn dismiss(key: &str) -> Result<Applied> {
    let db = JiraInbox::new()?;
    if !db.set_dismissed(key, true)? {
        return Ok(Applied::NotFound);
    }
    Ok(Applied::Dismissed)
}
