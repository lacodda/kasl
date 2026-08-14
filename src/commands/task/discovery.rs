//! `kasl task find`: gathers candidates from incomplete local tasks,
//! today's Jira resolutions and GitLab commits, deduplicates them, and
//! walks the user through importing (or permanently ignoring) them.

use crate::{
    api::{gitlab::GitLab, jira::Jira},
    db::tasks::Tasks,
    libs::{
        config::Config,
        messages::Message,
        prompt::ensure_interactive,
        task::{Task, TaskFilter, is_ignored_name, normalize_task_name},
    },
    msg_error, msg_print, msg_success, msg_warning,
};
use anyhow::Result;
use chrono::Local;
use dialoguer::{Input, MultiSelect, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Enumeration for identifying task suggestion sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TaskSource {
    /// Previously created but incomplete local tasks
    Incomplete,
    /// Commits from GitLab repositories for the current day
    Gitlab,
    /// Completed issues from Jira for the current day
    Jira,
}

/// Candidate discovered from a single source before UI presentation.
#[derive(Debug, Clone)]
struct DiscoveryItem {
    source: TaskSource,
    task: Task,
    /// Short GitLab commit SHA when available (display only)
    short_sha: Option<String>,
}

impl TaskSource {
    /// Lower value = higher priority when deduplicating by normalized name.
    fn priority(self) -> u8 {
        match self {
            TaskSource::Incomplete => 0,
            TaskSource::Jira => 1,
            TaskSource::Gitlab => 2,
        }
    }
}

fn format_discovery_item(item: &DiscoveryItem) -> String {
    match item.source {
        TaskSource::Incomplete => {
            format!("↻ {} — {}%", item.task.name, item.task.completeness.unwrap_or(0))
        }
        TaskSource::Jira => format!("◉ {}", item.task.name),
        TaskSource::Gitlab => match &item.short_sha {
            Some(sha) => format!("● {} ({})", item.task.name, sha),
            None => format!("● {}", item.task.name),
        },
    }
}

/// Keeps one item per normalized name, preferring Incomplete > Jira > GitLab.
fn dedup_discovery_items(items: Vec<DiscoveryItem>) -> Vec<DiscoveryItem> {
    let mut best: HashMap<String, DiscoveryItem> = HashMap::new();

    for item in items {
        let key = normalize_task_name(&item.task.name);
        match best.get(&key) {
            Some(existing) if existing.source.priority() <= item.source.priority() => {}
            _ => {
                best.insert(key, item);
            }
        }
    }

    let mut result: Vec<DiscoveryItem> = best.into_values().collect();
    result.sort_by(|a, b| a.source.priority().cmp(&b.source.priority()).then_with(|| a.task.name.cmp(&b.task.name)));
    result
}

/// Handles intelligent task discovery from multiple sources.
///
/// Aggregates incomplete local tasks, today's GitLab commits, and completed Jira
/// issues into a single filtered MultiSelect. Shows a spinner while fetching,
/// deduplicates near-identical names, and prioritizes incomplete tasks above
/// external imports.
pub(super) async fn handle_task_discovery(date: chrono::DateTime<Local>) -> Result<()> {
    // Discovery ends in a MultiSelect of what to import.
    ensure_interactive("`kasl task find` is interactive and needs a terminal")?;

    let date_naive = date.date_naive();
    let mut config = Config::read()?;
    let gitlab_config = config.gitlab.clone();
    let jira_config = config.jira.clone();
    let ignore_names = config.effective_ignore_names();

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner.set_message(Message::TasksDiscoverySearchingIncomplete.to_string());

    let incomplete_tasks = Tasks::new()?.fetch(TaskFilter::Incomplete)?;
    let today_tasks = Tasks::new()?.fetch(TaskFilter::Date(date_naive))?;
    let today_names: HashSet<String> = today_tasks.iter().map(|t| normalize_task_name(&t.name)).collect();

    spinner.set_message(Message::TasksDiscoveryFetchingExternal.to_string());

    let (commits_result, jira_result) = tokio::join!(
        async {
            match gitlab_config {
                Some(cfg) => GitLab::new(&cfg).get_today_commits().await,
                None => Ok(Vec::new()),
            }
        },
        async {
            match jira_config {
                Some(cfg) => {
                    let mut jira = Jira::new(&cfg);
                    jira.get_completed_issues(&date_naive).await
                }
                None => Ok(Vec::new()),
            }
        },
    );

    spinner.finish_and_clear();

    let commits = match commits_result {
        Ok(c) => c,
        Err(e) => {
            msg_warning!(Message::GitlabFetchFailed(e.to_string()));
            Vec::new()
        }
    };
    let jira_issues = match jira_result {
        Ok(issues) => issues,
        Err(e) => {
            msg_warning!(Message::JiraFetchFailed(e.to_string()));
            Vec::new()
        }
    };

    let mut candidates: Vec<DiscoveryItem> = Vec::new();

    for task in incomplete_tasks {
        if is_ignored_name(&task.name, &ignore_names) {
            continue;
        }
        if today_names.contains(&normalize_task_name(&task.name)) {
            continue;
        }
        candidates.push(DiscoveryItem {
            source: TaskSource::Incomplete,
            task,
            short_sha: None,
        });
    }

    for issue in jira_issues {
        let name = format!("{} {}", issue.key, issue.fields.summary);
        if is_ignored_name(&name, &ignore_names) {
            continue;
        }
        if today_names.contains(&normalize_task_name(&name)) {
            continue;
        }
        candidates.push(DiscoveryItem {
            source: TaskSource::Jira,
            task: Task::new(&name, "", Some(100)),
            short_sha: None,
        });
    }

    for commit in commits {
        if is_ignored_name(&commit.message, &ignore_names) {
            continue;
        }
        if today_names.contains(&normalize_task_name(&commit.message)) {
            continue;
        }
        let short_sha = if commit.sha.len() >= 7 {
            Some(commit.sha[..7].to_string())
        } else if commit.sha.is_empty() {
            None
        } else {
            Some(commit.sha.clone())
        };
        candidates.push(DiscoveryItem {
            source: TaskSource::Gitlab,
            task: Task::new(&commit.message, "", Some(100)),
            short_sha,
        });
    }

    let items = dedup_discovery_items(candidates);

    if items.is_empty() {
        msg_error!(Message::TasksNotFoundSad);
        return Ok(());
    }

    let incomplete_count = items.iter().filter(|i| i.source == TaskSource::Incomplete).count();
    let jira_count = items.iter().filter(|i| i.source == TaskSource::Jira).count();
    let gitlab_count = items.iter().filter(|i| i.source == TaskSource::Gitlab).count();

    msg_print!(
        Message::TasksDiscoverySummary {
            incomplete: incomplete_count,
            jira: jira_count,
            gitlab: gitlab_count,
        },
        true
    );

    // Build one MultiSelect: incomplete first, optional separator, then the rest.
    let mut labels: Vec<String> = Vec::new();
    let mut index_map: Vec<Option<usize>> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        if item.source == TaskSource::Incomplete {
            labels.push(format_discovery_item(item));
            index_map.push(Some(idx));
        }
    }

    let has_rest = items.iter().any(|i| i.source != TaskSource::Incomplete);
    if incomplete_count > 0 && has_rest {
        labels.push(Message::TasksDiscoverySeparator.to_string());
        index_map.push(None);
    }

    for (idx, item) in items.iter().enumerate() {
        if item.source != TaskSource::Incomplete {
            labels.push(format_discovery_item(item));
            index_map.push(Some(idx));
        }
    }

    let selected = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptSelectTasksToImport.to_string())
        .items(&labels)
        .interact()
        .unwrap_or_default();

    for sel in selected {
        let Some(item_idx) = index_map.get(sel).copied().flatten() else {
            continue; // separator or out of range
        };
        let Some(item) = items.get(item_idx) else {
            continue;
        };

        let mut task = item.task.clone();

        if item.source == TaskSource::Incomplete {
            msg_print!(Message::SelectingTask(task.name.clone()));

            if task.task_id.is_none() || task.task_id.is_some_and(|id| id == 0) {
                task.task_id = task.id;
            }

            let default_completeness = (task.completeness.unwrap_or(0) + 1).min(100);
            task.completeness = Some(
                Input::with_theme(&ColorfulTheme::default())
                    .allow_empty(true)
                    .with_prompt(Message::PromptTaskCompleteness.to_string())
                    .default(default_completeness)
                    .interact_text()
                    .unwrap(),
            );
        }

        let _ = Tasks::new()?.insert(&task);
    }

    // Optional: add selected discovery items to the persistent ignore list.
    let ignore_selected = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptSelectTasksToIgnore.to_string())
        .items(&labels)
        .interact()
        .unwrap_or_default();

    if !ignore_selected.is_empty() {
        let mut names_to_ignore = Vec::new();
        for sel in ignore_selected {
            let Some(item_idx) = index_map.get(sel).copied().flatten() else {
                continue;
            };
            if let Some(item) = items.get(item_idx) {
                names_to_ignore.push(item.task.name.clone());
            }
        }

        if !names_to_ignore.is_empty() {
            let added = config.add_ignore_names(&names_to_ignore)?;
            if added > 0 {
                msg_success!(Message::TaskDiscoveryIgnoreNamesAdded(added));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_prefers_incomplete_over_jira_over_gitlab() {
        let items = vec![
            DiscoveryItem {
                source: TaskSource::Gitlab,
                task: Task::new("New commit.", "", Some(100)),
                short_sha: Some("abc1234".into()),
            },
            DiscoveryItem {
                source: TaskSource::Jira,
                task: Task::new("New commit", "", Some(100)),
                short_sha: None,
            },
            DiscoveryItem {
                source: TaskSource::Incomplete,
                task: Task::new(" New commit", "", Some(40)),
                short_sha: None,
            },
        ];

        let deduped = dedup_discovery_items(items);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].source, TaskSource::Incomplete);
        assert_eq!(deduped[0].task.name, "New commit");
    }
}
