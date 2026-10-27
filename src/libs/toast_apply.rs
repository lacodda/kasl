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

use crate::db::inbox::Inbox;
use crate::db::tasks::Tasks;
use crate::libs::inbox_filter;
use crate::libs::inbox_rules;
use crate::libs::inbox_source;
use crate::libs::jira_bridge::{Goal, MoveOptions, Moved};
use crate::libs::task::{Task, TaskFilter};
use crate::libs::toast_action::{ToastAction, ToastRequest};
use anyhow::Result;
use chrono::{Local, NaiveDateTime};

/// What performing a decision came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// The issue became a task.
    ///
    /// `started` is what happened in Jira when `start_on_take` asked for the
    /// issue to be started there too: the move, or why it did not happen.
    /// `None` when nobody asked. `tags` are the ones the inbox rules gave the
    /// new task.
    Taken {
        started: Option<Result<Moved, String>>,
        tags: Vec<String>,
    },
    /// The issue was already a task; the name it carries.
    AlreadyTaken(String),
    /// The issue is asleep until this moment.
    Snoozed(NaiveDateTime),
    /// The issue was hidden from the list.
    Dismissed,
    /// The task taken from the issue is now done; its name.
    Completed(String),
    /// The task taken from the issue was done already; its name.
    AlreadyDone(String),
    /// The issue never became a task, so there is nothing to mark done.
    NoTask,
    /// No such key in the inbox - it was synced away between the toast and
    /// the click.
    NotFound,
}

impl Applied {
    /// One line for a toast: what happened, from the user's side.
    pub fn summary(&self, key: &str) -> String {
        match self {
            Applied::Taken { started, tags } => {
                let tagged = if tags.is_empty() {
                    String::new()
                } else {
                    format!(" tagged {}", tags.join(", "))
                };
                match started {
                    None => format!("{key} is now a task{tagged}"),
                    Some(Ok(moved)) => format!("{key} is now a task{tagged}; {}", moved.sentence(key, Goal::Start)),
                    Some(Err(why)) => format!("{key} is now a task{tagged}; {why}"),
                }
            }
            Applied::AlreadyTaken(name) => format!("{key} was already taken as '{name}'"),
            Applied::Snoozed(until) => format!("{key} sleeps until {}", until.format("%b %-d %H:%M")),
            Applied::Dismissed => format!("{key} dismissed"),
            Applied::Completed(name) => format!("{} is done", task_named(key, name)),
            Applied::AlreadyDone(name) => format!("{} was already done", task_named(key, name)),
            Applied::NoTask => format!("{key} was never taken as a task"),
            Applied::NotFound => format!("{key} is no longer in the inbox"),
        }
    }

    /// Whether the issue is settled and its buttons should stop existing.
    pub fn settled(&self) -> bool {
        !matches!(self, Applied::NotFound)
    }
}

/// A task by name, with the issue key in front unless the name already opens with it.
///
/// A task taken from the inbox is named `KEY summary`, so the key is usually
/// there already; a renamed one may have lost it, and the answer must still
/// say which issue it is about.
fn task_named(key: &str, name: &str) -> String {
    if name.starts_with(key) {
        format!("'{name}'")
    } else {
        format!("{key} '{name}'")
    }
}

/// Performs one request, whichever action it carries.
///
/// The snooze window comes from the config so a button and `inbox triage`
/// sleep for the same length; an unreadable window falls back to a day rather
/// than failing the click, because a button that silently does nothing is
/// worse than one that sleeps for the default.
///
/// A button has nobody to ask, so a take that starts the issue in Jira does
/// it unattended: a cached session or a stored password, and no choosing
/// between transitions.
pub async fn apply(request: &ToastRequest) -> Result<Applied> {
    match request.action {
        ToastAction::Take => take(&request.issue_key, &MoveOptions::unattended()).await,
        ToastAction::Snooze => {
            let window = snooze_window();
            snooze(&request.issue_key, window)
        }
        ToastAction::Dismiss => dismiss(&request.issue_key),
        ToastAction::Done => complete(&request.issue_key),
    }
}

/// How long a toast's Snooze button sleeps.
pub fn snooze_window() -> chrono::Duration {
    let configured = crate::libs::config::Config::read().ok().and_then(|c| c.inbox).map(|i| i.toast_snooze_for);
    match configured.as_deref().map(inbox_filter::parse_window) {
        Some(Ok(window)) => window,
        _ => chrono::Duration::days(1),
    }
}

/// Whether a take also starts the issue in Jira (`inbox.start_on_take`).
fn start_on_take() -> bool {
    crate::libs::config::Config::read()
        .ok()
        .and_then(|c| c.inbox)
        .is_some_and(|inbox| inbox.start_on_take)
}

/// Turns an inbox issue into a task, as `kasl inbox take` does.
///
/// Taking the same issue twice must not fan out into duplicate tasks: from a
/// toast that is even likelier than from the shell, because a toast can be
/// clicked from the notification centre long after it appeared.
///
/// The task is made first and stands whatever Jira says next. Starting the
/// issue is a second, outward step: a VPN that is down must not cost the
/// person the task they asked for, so a failed start is reported alongside the
/// take rather than instead of it. A second take of the same issue does not
/// start it again - the first take's answer said how to.
pub async fn take(key: &str, options: &MoveOptions) -> Result<Applied> {
    let applied = take_locally(key)?;
    let Applied::Taken { tags, .. } = applied else {
        return Ok(applied);
    };
    if !start_on_take() {
        return Ok(Applied::Taken { started: None, tags });
    }
    let at = inbox_source::label(&inbox_source::source_of(key)?);
    let started = inbox_source::move_issue(key, Goal::Start, options)
        .await
        .map_err(|e| format!("{at} did not start it: {e:#}; `kasl inbox start {key}` tries again"));
    Ok(Applied::Taken { started: Some(started), tags })
}

/// The local half of a take: the task, its tags, and the mark on the issue.
///
/// The tags come from the inbox rules that match the issue as it is now, so a
/// project rule tags the task whichever way it was taken.
fn take_locally(key: &str) -> Result<Applied> {
    let db = Inbox::new()?;
    let Some(item) = db.get_by_key(key)? else {
        return Ok(Applied::NotFound);
    };

    let mut tasks = Tasks::new()?;
    if let Some(task) = tasks.fetch(TaskFilter::ByIssueKey(key.to_string()))?.first() {
        // Repair the mark if only the task survived.
        if item.taken_at.is_none() {
            let _ = db.set_taken(key, true)?;
        }
        return Ok(Applied::AlreadyTaken(task.name.clone()));
    }

    let name = format!("{} {}", item.issue_key, item.summary);
    let task = Task::new(&name, "", Some(0)).from_issue(key);
    tasks.insert(&task)?;
    let _ = db.set_taken(key, true)?;
    let wanted = inbox_rules::tags_for(&item);
    let tags = if wanted.is_empty() {
        Vec::new()
    } else {
        inbox_rules::tag_task(key, &wanted)?
    };
    Ok(Applied::Taken { started: None, tags })
}

/// Marks the task taken from `key` done.
///
/// A task is a chain of daily rows, each saying how far along it was that
/// day. Done means the newest row says 100: today's row is raised when there
/// is one, otherwise a row for today continues the chain - the same thing
/// `task find` does when work carries over, so the report for today shows the
/// task finished today.
pub fn complete(key: &str) -> Result<Applied> {
    let mut tasks = Tasks::new()?;
    let rows = tasks.fetch(TaskFilter::ByIssueKey(key.to_string()))?;
    let Some(latest) = latest_row(&rows) else {
        return Ok(Applied::NoTask);
    };
    if latest.completeness == Some(100) {
        return Ok(Applied::AlreadyDone(latest.name.clone()));
    }

    let today = Local::now().format("%Y-%m-%d").to_string();
    if latest.timestamp.as_deref().is_some_and(|stamp| stamp.starts_with(&today)) {
        let mut row = latest.clone();
        row.completeness = Some(100);
        tasks.update(&row)?;
    } else {
        let mut row = Task::new(&latest.name, &latest.comment, Some(100)).from_issue(key);
        row.task_id = latest.task_id.filter(|&chain| chain != 0).or(latest.id);
        tasks.insert(&row)?;
    }
    Ok(Applied::Completed(latest.name.clone()))
}

/// Whether the task taken from `key` is still unfinished.
pub fn task_open(key: &str) -> Result<bool> {
    let rows = Tasks::new()?.fetch(TaskFilter::ByIssueKey(key.to_string()))?;
    Ok(latest_row(&rows).is_some_and(|row| row.completeness != Some(100)))
}

/// The newest row of a task's chain: latest stamp, then highest id.
fn latest_row(rows: &[Task]) -> Option<&Task> {
    rows.iter().max_by(|a, b| a.timestamp.cmp(&b.timestamp).then(a.id.cmp(&b.id)))
}

/// Puts an issue to sleep for `window`.
pub fn snooze(key: &str, window: chrono::Duration) -> Result<Applied> {
    let until = Local::now().naive_local() + window;
    let db = Inbox::new()?;
    if !db.set_snoozed(key, Some(until))? {
        return Ok(Applied::NotFound);
    }
    Ok(Applied::Snoozed(until))
}

/// Hides an issue from the list.
pub fn dismiss(key: &str) -> Result<Applied> {
    let db = Inbox::new()?;
    if !db.set_dismissed(key, true)? {
        return Ok(Applied::NotFound);
    }
    Ok(Applied::Dismissed)
}
