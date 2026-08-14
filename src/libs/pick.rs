//! Interactive fallbacks for arguments the user did not pass.
//!
//! The rule across the CLI: on a terminal a missing identifier opens a picker,
//! and everywhere else the command fails exactly as it did before, so scripts
//! and CI keep their old behaviour. A picker is a convenience for the human at
//! the keyboard, never a new way for an unattended run to hang.
//!
//! Every picker here calls [`ensure_interactive`] first, for the reason spelled
//! out in [`crate::libs::prompt`]: `dialoguer` reads stdin unconditionally and
//! would otherwise block forever, or read EOF and report an empty answer as if
//! the user had chosen nothing.
//!
//! The labels matter as much as the list. Picking an issue by key alone means
//! reading `PROJ-4471` and guessing; the labels here carry the same summary,
//! badge and duration the list views show, so the choice is made on what the
//! row means rather than on its identifier.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::pick;
//! use kasl::db::jira_inbox::JiraInbox;
//!
//! # fn main() -> anyhow::Result<()> {
//! // A command with an optional KEY resolves it like this.
//! let key = match None::<String> {
//!     Some(key) => key,
//!     None => pick::inbox_issue(&JiraInbox::new()?.list_active(false)?, "Pick an issue")?,
//! };
//! # Ok(())
//! # }
//! ```

use anyhow::{Result, bail};
use chrono::Local;
use dialoguer::{MultiSelect, Select, theme::ColorfulTheme};

use crate::db::jira_inbox::JiraInboxItem;
use crate::db::tags::Tag;
use crate::db::templates::TaskTemplate;
use crate::libs::pause::Pause;
use crate::libs::prompt::ensure_interactive;
use crate::libs::task::Task;

/// Shows `prompt` over `labels` and returns the index the user picked.
fn select(prompt: &str, labels: &[String]) -> Result<usize> {
    Ok(Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(labels)
        .default(0)
        .interact()?)
}

/// Picks an inbox issue, showing what each one is rather than just its key.
///
/// Returns the issue key. An empty inbox is a refusal rather than an empty
/// picker: there is nothing to choose, and saying so names the command that
/// would fill it.
pub fn inbox_issue(items: &[JiraInboxItem], prompt: &str) -> Result<String> {
    if items.is_empty() {
        bail!("the inbox is empty - run `kasl inbox sync` to fetch assigned issues");
    }
    ensure_interactive("issue key is required; pass KEY outside a terminal")?;

    let now = Local::now().naive_local();
    let width = items.iter().map(|i| i.issue_key.len()).max().unwrap_or(0);
    let labels: Vec<String> = items
        .iter()
        .map(|item| {
            let pin = if item.pinned { "*" } else { " " };
            let badge = item.badge(now).map(|b| format!("  [{b}]")).unwrap_or_default();
            format!("{pin}{:width$}  {}{badge}", item.issue_key, item.summary)
        })
        .collect();

    Ok(items[select(prompt, &labels)?].issue_key.clone())
}

/// Picks one or more tasks, showing name and completeness.
///
/// Returns the chosen ids. Multi-select because the commands that need this
/// accept several ids, and picking them one at a time would be worse than
/// typing them.
pub fn tasks(tasks: &[Task], prompt: &str) -> Result<Vec<i32>> {
    if tasks.is_empty() {
        bail!("no tasks to choose from - `kasl task add` creates one");
    }
    ensure_interactive("task id is required; pass ID outside a terminal")?;

    let labels: Vec<String> = tasks
        .iter()
        .map(|t| format!("[{}] {} ({}%)", t.id.unwrap_or(0), t.name, t.completeness.unwrap_or(0)))
        .collect();

    let chosen = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(&labels)
        .interact()?;

    Ok(chosen.into_iter().filter_map(|i| tasks[i].id).collect())
}

/// Picks a template by name, showing the task title it would create.
pub fn template(templates: &[TaskTemplate], prompt: &str) -> Result<String> {
    if templates.is_empty() {
        bail!("no templates yet - run `kasl template add` first");
    }
    ensure_interactive("template name is required; pass NAME outside a terminal")?;

    let width = templates.iter().map(|t| t.name.len()).max().unwrap_or(0);
    let labels: Vec<String> = templates.iter().map(|t| format!("{:width$}  {}", t.name, t.task_name)).collect();

    Ok(templates[select(prompt, &labels)?].name.clone())
}

/// Picks a tag by name, showing its colour where one is set.
///
/// Returns the tag name, which is what `tag edit` and `tag remove` accept
/// alongside a numeric id.
pub fn tag(tags: &[Tag], prompt: &str) -> Result<String> {
    if tags.is_empty() {
        bail!("no tags yet - run `kasl tag add` first");
    }
    ensure_interactive("tag name is required; pass TAG outside a terminal")?;

    let width = tags.iter().map(|t| t.name.len()).max().unwrap_or(0);
    let labels: Vec<String> = tags
        .iter()
        .map(|t| match &t.color {
            Some(color) if !color.is_empty() => format!("{:width$}  {color}", t.name),
            _ => t.name.clone(),
        })
        .collect();

    Ok(tags[select(prompt, &labels)?].name.clone())
}

/// Picks a pause, showing when it started and how long it lasted.
///
/// Returns the pause id. Pause ids are database keys nobody memorises, which
/// is exactly why removing one by hand needs this.
pub fn pause(pauses: &[Pause], prompt: &str) -> Result<i32> {
    if pauses.is_empty() {
        bail!("no pauses recorded for that day - `kasl pauses list` shows what is there");
    }
    ensure_interactive("pause id is required; pass ID outside a terminal")?;

    let labels: Vec<String> = pauses
        .iter()
        .map(|p| {
            let start = p.start.format("%H:%M");
            let end = p.end.map(|e| e.format("%H:%M").to_string()).unwrap_or_else(|| "…".to_string());
            match p.duration {
                Some(d) => format!("{start} - {end}  ({} min)", d.num_minutes()),
                None => format!("{start} - {end}  (ongoing)"),
            }
        })
        .collect();

    Ok(pauses[select(prompt, &labels)?].id)
}
