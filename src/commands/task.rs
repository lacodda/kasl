//! Task management command.
//!
//! Provides comprehensive task management functionality for creating, editing, deleting, and organizing tasks.
//!
//! ## Features
//!
//! - **CRUD Operations**: Create, read, update, and delete individual tasks
//! - **Batch Operations**: Mass editing and deletion of multiple tasks
//! - **External Integration**: Import tasks from GitLab commits and Jira issues
//! - **Advanced Filtering**: View tasks by date, completion status, tags, or IDs
//! - **Template System**: Create tasks from predefined templates
//!
//! ## Usage
//!
//! ```bash
//! # Create a new task
//! kasl task --name "Review code" --comment "Check PR #123"
//!
//! # List tasks with different filters
//! kasl task --list                    # Show all tasks
//! kasl task --today                   # Show today's tasks
//! kasl task --incomplete              # Show incomplete tasks
//!
//! # Import from external services
//! kasl task --find                    # Find tasks from GitLab/Jira
//! kasl task --template "bug-fix"      # Create from template
//! ```

use crate::{
    api::{gitlab::GitLab, jira::Jira},
    db::tasks::Tasks,
    db::templates::Templates,
    libs::{
        config::Config,
        messages::Message,
        stdin_drain::drain_available_stdin_lines,
        task::{collapse_whitespace, is_ignored_name, normalize_task_name, Task, TaskFilter},
        view::View,
    },
    msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use chrono::Local;
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Enumeration for identifying task suggestion sources.
///
/// This enum helps distinguish between different sources of task suggestions
/// during the interactive task finding process, allowing for appropriate
/// handling and user feedback for each source type.
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
            format!(
                "↻ {} — {}%",
                item.task.name,
                item.task.completeness.unwrap_or(0)
            )
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
    result.sort_by(|a, b| {
        a.source
            .priority()
            .cmp(&b.source.priority())
            .then_with(|| a.task.name.cmp(&b.task.name))
    });
    result
}

/// Command-line arguments for comprehensive task management.
///
/// The task command supports extensive functionality through various argument
/// combinations, enabling both simple task creation and complex task management
/// operations including batch processing and external integrations.
#[derive(Debug, Args)]
pub struct TaskArgs {
    /// Name or title of the task to create
    ///
    /// When creating a new task, this specifies the primary identifier
    /// and description. Task names should be descriptive enough to
    /// understand the work involved at a glance.
    #[arg(short, long)]
    name: Option<String>,

    /// Optional descriptive comment for additional task context
    ///
    /// Provides space for detailed descriptions, requirements, or notes
    /// about the task. This field supports longer text and can include
    /// technical details, links, or implementation notes.
    #[arg(long)]
    comment: Option<String>,

    /// Task completion percentage (0-100)
    ///
    /// Indicates how much of the task has been completed:
    /// - 0: Not started
    /// - 1-99: In progress with specific completion level
    /// - 100: Fully completed
    ///
    /// This enables tracking of partial task completion and work-in-progress items.
    #[arg(short, long)]
    completeness: Option<i32>,

    /// Display existing tasks with optional filtering
    ///
    /// When specified, shows tasks instead of creating new ones. Can be
    /// combined with other filtering options like --all, --id, or --tag
    /// to control which tasks are displayed.
    #[arg(short, long)]
    show: bool,

    /// Show all tasks regardless of date (use with --show)
    ///
    /// Overrides the default behavior of showing only today's tasks,
    /// displaying the complete task history. Useful for reviewing
    /// long-term work patterns and finding historical tasks.
    #[arg(short, long)]
    all: bool,

    /// Display or operate on specific task IDs (use with --show)
    ///
    /// Accepts a list of task IDs for precise task selection. This enables
    /// targeted operations on specific tasks without filtering through
    /// the entire task list.
    #[arg(short, long)]
    id: Option<Vec<i32>>,

    /// Find and suggest tasks from multiple sources
    ///
    /// Activates the intelligent task discovery mode that:
    /// - Finds incomplete local tasks that can be continued
    /// - Imports today's commits from configured GitLab repositories
    /// - Retrieves completed issues from configured Jira instances
    /// - Presents unified interface for task selection and creation
    #[arg(short, long, help = "Find incomplete tasks")]
    find: bool,

    /// Delete specified tasks by their IDs
    ///
    /// Accepts a list of task IDs for permanent removal. This operation
    /// includes confirmation prompts and shows preview of tasks to be
    /// deleted for safety.
    #[arg(short, long, help = "Delete tasks by IDs")]
    delete: Option<Vec<i32>>,

    /// Delete all tasks for today (requires confirmation)
    ///
    /// Dangerous operation that removes all tasks created today.
    /// Includes multiple confirmation steps to prevent accidental
    /// data loss. Use with extreme caution.
    #[arg(long, help = "Delete all tasks for today")]
    delete_today: bool,

    /// Edit a specific task by its ID
    ///
    /// Opens interactive editing interface for the specified task,
    /// allowing modification of name, comment, and completion status.
    /// Includes preview of changes before applying.
    #[arg(short, long, help = "Edit task by ID")]
    edit: Option<i32>,

    /// Interactive batch editing of multiple tasks
    ///
    /// Presents a selection interface for choosing multiple tasks
    /// from today's list, then provides editing interface for each
    /// selected task. Efficient for updating multiple related tasks.
    #[arg(long, help = "Edit multiple tasks interactively")]
    edit_interactive: bool,

    /// Create task from a named template
    ///
    /// Uses a predefined task template to create a new task with
    /// default values. Template values can be modified during creation.
    /// Streamlines creation of frequently used task types.
    #[arg(long, short = 't')]
    template: Option<String>,

    /// Interactive template selection for task creation
    ///
    /// Displays available templates and allows selection through
    /// a user-friendly interface. Alternative to specifying template
    /// name directly via --template flag.
    #[arg(long, short = 'l')]
    from_template: bool,

    /// Comma-separated list of tags to apply to the task
    ///
    /// Assigns categorization tags to the task for organization
    /// and filtering. Tags will be created automatically if they
    /// don't exist. Example: "urgent,backend,bug"
    #[arg(long)]
    tags: Option<String>,

    /// Filter tasks by a specific tag name (use with --show)
    ///
    /// Displays only tasks that have been assigned the specified tag.
    /// Useful for viewing tasks within a specific category or project.
    #[arg(long)]
    tag: Option<String>,
}

/// Main entry point for the comprehensive task management command.
///
/// This function serves as a large dispatcher that handles the various task management
/// operations based on provided command-line flags. It supports everything from simple
/// task creation to complex batch operations and external service integrations.
///
/// ## Operation Modes
///
/// The function handles these primary modes:
///
/// 1. **Deletion Operations**: Remove tasks individually or in bulk
/// 2. **Editing Operations**: Modify existing tasks individually or in batches
/// 3. **Template Operations**: Create tasks from predefined templates
/// 4. **Display Operations**: Show tasks with various filtering options
/// 5. **Discovery Operations**: Find and import tasks from multiple sources
/// 6. **Creation Operations**: Create new tasks manually or interactively
///
/// ## External Integrations
///
/// When find mode is activated, the function integrates with:
/// - **GitLab API**: Fetches today's commits as potential completed tasks
/// - **Jira API**: Retrieves completed issues for the current day
/// - **Local Database**: Finds incomplete tasks that can be continued
///
/// ## Safety Features
///
/// Destructive operations include multiple safety measures:
/// - Preview of changes before applying
/// - Multiple confirmation prompts for bulk operations
/// - Detailed information about affected items
/// - Option to cancel operations at multiple points
///
/// # Arguments
///
/// * `task_args` - Parsed command-line arguments specifying the operation to perform
///
/// # Returns
///
/// Returns `Ok(())` on successful operation completion, or an error if the
/// requested operation fails due to validation, database, or network issues.
///
/// # Examples
///
/// ```bash
/// # Create a simple task
/// kasl task --name "Review pull request" --completeness 0
///
/// # Create task with tags
/// kasl task --name "Fix login bug" --tags "urgent,backend,bug"
///
/// # Find and import tasks from external sources
/// kasl task --find
///
/// # Show all tasks with specific tag
/// kasl task --show --tag urgent
///
/// # Edit multiple tasks interactively
/// kasl task --edit-interactive
///
/// # Create task from template
/// kasl task --template daily-standup
///
/// # Delete specific tasks (with confirmation)
/// kasl task --delete 1 2 3
/// ```
pub async fn cmd(task_args: TaskArgs) -> Result<()> {
    let date = Local::now();

    // Handle deletion operations first (highest priority for safety)
    if let Some(ids) = task_args.delete {
        return handle_delete_by_ids(ids).await;
    }

    if task_args.delete_today {
        return handle_delete_today().await;
    }

    // Handle editing operations
    if let Some(id) = task_args.edit {
        return handle_edit_by_id(id).await;
    }

    if task_args.edit_interactive {
        return handle_edit_interactive().await;
    }

    // Handle template-based creation
    if let Some(template_name) = task_args.template {
        return handle_create_from_template(template_name).await;
    }

    if task_args.from_template {
        return handle_create_from_template_interactive().await;
    }

    // Handle display operations
    if task_args.show {
        let mut filter: TaskFilter = TaskFilter::Date(date.date_naive());

        // Apply appropriate filter based on arguments
        if task_args.all {
            filter = TaskFilter::All;
        } else if task_args.id.is_some() {
            filter = TaskFilter::ByIds(task_args.id.unwrap());
        } else if let Some(tag) = task_args.tag {
            filter = TaskFilter::ByTag(tag);
        }

        let tasks = Tasks::new()?.fetch(filter)?;
        if tasks.is_empty() {
            msg_error!(Message::TaskNotFound);
            return Ok(());
        }
        View::tasks(&tasks)?;

        return Ok(());
    } else if task_args.find {
        // Handle intelligent task discovery and import
        return handle_task_discovery(date).await;
    }

    // Default: Create new task (manual or interactive)
    handle_task_creation(task_args).await
}

/// Handles intelligent task discovery from multiple sources.
///
/// Aggregates incomplete local tasks, today's GitLab commits, and completed Jira
/// issues into a single filtered MultiSelect. Shows a spinner while fetching,
/// deduplicates near-identical names, and prioritizes incomplete tasks above
/// external imports.
///
/// # Arguments
///
/// * `date` - Current date/time for filtering today's external content
async fn handle_task_discovery(date: chrono::DateTime<Local>) -> Result<()> {
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

    let (commits, jira_issues) = tokio::join!(
        async {
            match gitlab_config {
                Some(cfg) => GitLab::new(&cfg).get_today_commits().await.unwrap_or_default(),
                None => Vec::new(),
            }
        },
        async {
            match jira_config {
                Some(cfg) => {
                    let mut jira = Jira::new(&cfg);
                    jira.get_completed_issues(&date_naive).await.unwrap_or_default()
                }
                None => Vec::new(),
            }
        },
    );

    spinner.finish_and_clear();

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
    use crate::libs::config::default_ignore_names;

    #[test]
    fn normalize_collapses_whitespace_and_punctuation() {
        assert_eq!(normalize_task_name("New commit"), "new commit");
        assert_eq!(normalize_task_name("New commit "), "new commit");
        assert_eq!(normalize_task_name("New commit."), "new commit");
        assert_eq!(normalize_task_name(" New commit"), "new commit");
        assert_eq!(normalize_task_name("New  commit..."), "new commit");
    }

    #[test]
    fn collapse_whitespace_turns_newlines_into_spaces() {
        let pasted = "PROJ-42\nFix login redirect for OAuth callback\r\n";
        assert_eq!(
            collapse_whitespace(pasted),
            "PROJ-42 Fix login redirect for OAuth callback"
        );
        assert_eq!(collapse_whitespace("  spaced   out  "), "spaced out");
    }

    #[test]
    fn looks_like_issue_key_detects_jira_keys() {
        assert!(looks_like_issue_key("PROJ-42"));
        assert!(looks_like_issue_key("ABC-1001"));
        assert!(!looks_like_issue_key("PROJ-42 summary"));
        assert!(!looks_like_issue_key("update alert"));
        assert!(!looks_like_issue_key(""));
    }

    #[test]
    fn default_ignore_filters_merge_and_update_webui_but_not_update_alert() {
        let ignore = default_ignore_names();
        assert!(!ignore.iter().any(|n| normalize_task_name(n) == "update alert"));

        assert!(is_ignored_name(
            "Merge remote-tracking branch 'origin/release/4.39.0' into feature/x",
            &ignore
        ));
        assert!(is_ignored_name("Merge branch 'main' into feature/x", &ignore));
        assert!(is_ignored_name("update webui", &ignore));
        assert!(is_ignored_name("  Update WebUI  ", &ignore));
        assert!(!is_ignored_name("update alert", &ignore));
        assert!(!is_ignored_name("Fix login validation", &ignore));
    }

    #[test]
    fn custom_ignore_list_filters_update_alert() {
        let mut ignore = default_ignore_names();
        ignore.push("update alert".to_string());
        assert!(is_ignored_name("update alert", &ignore));
        assert!(is_ignored_name("Update alert.", &ignore));
    }

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

/// Prompts for a task name and absorbs leftover multi-line paste from stdin.
fn prompt_task_name_interactive() -> String {
    let raw = crate::libs::stdin_drain::read_pastable_line(&Message::PromptTaskName.to_string())
        .unwrap_or_default();
    let name = collapse_whitespace(&raw);
    if raw.lines().filter(|l| !l.trim().is_empty()).count() > 1 {
        msg_info!(Message::TaskNameMergedFromPaste);
    }
    if !name.is_empty() {
        println!("✔ {}", name);
    }
    name
}

/// Returns true for bare issue keys like `PROJ-42` (typical first line of a ticket paste).
fn looks_like_issue_key(name: &str) -> bool {
    let name = name.trim();
    if name.is_empty() || name.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    let Some((prefix, rest)) = name.split_once('-') else {
        return false;
    };
    !prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_alphanumeric())
        && !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_digit())
}

/// Handles manual task creation with interactive prompts.
///
/// This function manages the standard task creation workflow, collecting
/// task information either from command-line arguments or interactive prompts.
/// It also handles tag assignment and provides immediate feedback about
/// the created task.
///
/// # Arguments
///
/// * `task_args` - Command-line arguments containing optional task information
async fn handle_task_creation(task_args: TaskArgs) -> Result<()> {
    // Collect task information (from args or interactive prompts)
    let name_from_args = task_args.name.is_some();
    let comment_from_args = task_args.comment.is_some();

    let mut name = match task_args.name {
        Some(n) => collapse_whitespace(&n),
        None => prompt_task_name_interactive(),
    };

    let mut comment = match task_args.comment {
        Some(c) => collapse_whitespace(&c),
        None => {
            let raw: String = Input::with_theme(&ColorfulTheme::default())
                .allow_empty(true)
                .with_prompt(Message::PromptTaskComment.to_string())
                .interact_text()
                .unwrap();
            collapse_whitespace(&raw)
        }
    };

    // Fallback: multi-line paste often lands as name=KEY + comment=summary when stdin
    // drain is unavailable (native console). Rejoin and ask for a real comment again.
    if !name_from_args && !comment_from_args && looks_like_issue_key(&name) && !comment.is_empty() {
        name = collapse_whitespace(&format!("{} {}", name, comment));
        msg_info!(Message::TaskNameMergedFromPaste);
        let raw: String = Input::with_theme(&ColorfulTheme::default())
            .allow_empty(true)
            .with_prompt(Message::PromptTaskComment.to_string())
            .interact_text()
            .unwrap();
        comment = collapse_whitespace(&raw);
    }

    // Discard any remaining paste leftovers before completeness.
    let _ = drain_available_stdin_lines();

    let completeness = task_args.completeness.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .allow_empty(true)
            .with_prompt(Message::PromptTaskCompleteness.to_string())
            .default(100)
            .interact_text()
            .unwrap()
    });

    // Create and insert the task
    let task = Task::new(&name, &comment, Some(completeness));
    let new_task = Tasks::new()?.insert(&task)?.update_id()?.get()?;
    View::tasks(&new_task)?;

    // Handle tag assignment if provided
    if let Some(tags_str) = task_args.tags {
        let tag_names: Vec<String> = tags_str.split(',').map(|s| s.trim().to_string()).collect();

        let mut tags_db = crate::db::tags::Tags::new()?;
        let tag_ids = tags_db.get_or_create_tags(&tag_names)?;

        if let Some(task_id) = new_task[0].id {
            tags_db.set_task_tags(task_id, &tag_ids)?;
            msg_info!(Message::TagsAddedToTask(tag_names.join(", ")));
        }
    }

    Ok(())
}

/// Handles deletion of multiple tasks by their IDs.
///
/// This function provides a safe deletion interface with preview and confirmation
/// for removing multiple tasks simultaneously. It includes validation to ensure
/// all specified task IDs exist before performing any deletions.
///
/// ## Safety Features
///
/// - Validates all task IDs exist before deletion
/// - Shows preview of tasks to be deleted
/// - Requires explicit user confirmation
/// - Provides clear feedback about deletion results
/// - Handles non-existent IDs gracefully
///
/// # Arguments
///
/// * `ids` - Vector of task IDs to delete
async fn handle_delete_by_ids(ids: Vec<i32>) -> Result<()> {
    if ids.is_empty() {
        msg_error!(Message::NoTaskIdsProvided);
        return Ok(());
    }

    let mut tasks_db = Tasks::new()?;

    // Fetch tasks to show preview of what will be deleted
    let tasks = tasks_db.fetch(TaskFilter::ByIds(ids.clone()))?;

    if tasks.is_empty() {
        msg_error!(Message::TasksNotFoundForIds(ids));
        return Ok(());
    }

    // Show preview of tasks to be deleted
    msg_print!(Message::TasksToBeDeleted, true);
    View::tasks(&tasks)?;

    // Request confirmation based on number of tasks
    let confirmed = if ids.len() == 1 {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::ConfirmDeleteTask.to_string())
            .default(false)
            .interact()?
    } else {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::ConfirmDeleteTasks(ids.len()).to_string())
            .default(false)
            .interact()?
    };

    if confirmed {
        let deleted_count = tasks_db.delete_many(&ids)?;
        msg_success!(Message::TasksDeletedCount(deleted_count));
    } else {
        msg_info!(Message::OperationCancelled);
    }

    Ok(())
}

/// Handles deletion of all tasks for today.
///
/// This is a dangerous operation that removes all tasks created today.
/// It includes multiple confirmation steps and detailed previews to
/// prevent accidental data loss.
///
/// ## Safety Measures
///
/// - Shows complete list of tasks to be deleted
/// - Requires two separate confirmations
/// - Uses clear warning language
/// - Defaults to "No" for all confirmations
/// - Provides escape points throughout the process
async fn handle_delete_today() -> Result<()> {
    let mut tasks_db = Tasks::new()?;
    let today = Local::now().date_naive();

    // Fetch today's tasks
    let tasks = tasks_db.fetch(TaskFilter::Date(today))?;

    if tasks.is_empty() {
        msg_info!(Message::NoTasksForToday);
        return Ok(());
    }

    // Show complete preview of tasks to be deleted
    msg_print!(Message::TasksToBeDeleted, true);
    View::tasks(&tasks)?;

    // First confirmation with task count
    let first_confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::ConfirmDeleteAllTodayTasks(tasks.len()).to_string())
        .default(false)
        .interact()?;

    if !first_confirm {
        msg_info!(Message::OperationCancelled);
        return Ok(());
    }

    // Second confirmation with stronger warning
    let second_confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::ConfirmDeleteAllTodayTasksFinal.to_string())
        .default(false)
        .interact()?;

    if second_confirm {
        let ids: Vec<i32> = tasks.iter().filter_map(|t| t.id).collect();
        let deleted_count = tasks_db.delete_many(&ids)?;
        msg_success!(Message::TasksDeletedCount(deleted_count));
    } else {
        msg_info!(Message::OperationCancelled);
    }

    Ok(())
}

/// Handles editing a single task by its ID.
///
/// Provides an interactive editing interface for modifying task properties
/// including name, comment, and completion status. Includes preview of
/// changes before applying them to the database.
///
/// # Arguments
///
/// * `id` - Database ID of the task to edit
async fn handle_edit_by_id(id: i32) -> Result<()> {
    let mut tasks_db = Tasks::new()?;

    // Fetch the task to edit
    let task = match tasks_db.get_by_id(id)? {
        Some(task) => task,
        None => {
            msg_error!(Message::TaskNotFoundWithId(id));
            return Ok(());
        }
    };

    // Show current task state
    msg_print!(Message::CurrentTaskState, true);
    View::tasks(&[task.clone()])?;

    // Interactive editing
    let edited_task = edit_task_interactive(&task)?;

    // Check if anything actually changed
    if edited_task.name == task.name && edited_task.comment == task.comment && edited_task.completeness == task.completeness {
        msg_info!(Message::NoChangesDetected);
        return Ok(());
    }

    // Show preview of changes
    msg_print!(Message::TaskEditPreview, true);
    View::tasks(&[edited_task.clone()])?;

    // Confirm changes
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::ConfirmTaskUpdate.to_string())
        .default(true)
        .interact()?;

    if confirmed {
        let mut task_to_update = task;
        task_to_update.update_from(&edited_task);
        tasks_db.update(&task_to_update)?;
        msg_success!(Message::TaskUpdated);
    } else {
        msg_info!(Message::OperationCancelled);
    }

    Ok(())
}

/// Handles interactive batch editing of multiple tasks.
///
/// Presents a selection interface for choosing multiple tasks from today's
/// list, then provides individual editing interfaces for each selected task.
/// This is efficient for updating multiple related tasks in sequence.
async fn handle_edit_interactive() -> Result<()> {
    let mut tasks_db = Tasks::new()?;

    // Get today's tasks for selection
    let today = Local::now().date_naive();
    let tasks = tasks_db.fetch(TaskFilter::Date(today))?;

    if tasks.is_empty() {
        msg_info!(Message::NoTasksForToday);
        return Ok(());
    }

    // Create selection list with task descriptions
    let task_descriptions: Vec<String> = tasks
        .iter()
        .map(|t| format!("[{}] {} ({}%)", t.id.unwrap_or(0), t.name, t.completeness.unwrap_or(0)))
        .collect();

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::SelectTasksToEdit.to_string())
        .items(&task_descriptions)
        .interact()?;

    if selections.is_empty() {
        msg_info!(Message::NoTasksSelected);
        return Ok(());
    }

    // Edit each selected task in sequence
    for &index in &selections {
        let task = &tasks[index];

        msg_print!(Message::EditingTask(task.name.clone()), true);
        View::tasks(&[task.clone()])?;

        let edited_task = edit_task_interactive(task)?;

        // Apply changes if anything was modified
        if edited_task.name != task.name || edited_task.comment != task.comment || edited_task.completeness != task.completeness {
            let mut task_to_update = task.clone();
            task_to_update.update_from(&edited_task);
            tasks_db.update(&task_to_update)?;
            msg_success!(Message::TaskUpdatedWithName(task.name.clone()));
        } else {
            msg_info!(Message::TaskSkippedNoChanges(task.name.clone()));
        }
    }

    msg_success!(Message::TaskEditingCompleted);
    Ok(())
}

/// Interactive task editing helper function.
///
/// Provides a consistent interactive interface for editing task properties.
/// Used by both single and batch editing operations to ensure uniform
/// user experience and validation.
///
/// # Arguments
///
/// * `task` - Original task to edit (used for default values)
///
/// # Returns
///
/// Returns a new Task instance with updated values from user input.
fn edit_task_interactive(task: &Task) -> Result<Task> {
    let name = collapse_whitespace(
        &Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptTaskNameEdit.to_string())
            .default(task.name.clone())
            .interact_text()?,
    );

    let comment = collapse_whitespace(
        &Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptTaskCommentEdit.to_string())
            .default(task.comment.clone())
            .allow_empty(true)
            .interact_text()?,
    );

    let completeness_range_msg = Message::TaskCompletenessRange.to_string();
    let completeness = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTaskCompletenessEdit.to_string())
        .default(task.completeness.unwrap_or(100))
        .validate_with(|input: &i32| -> Result<(), &str> {
            if *input >= 0 && *input <= 100 {
                Ok(())
            } else {
                Err(&completeness_range_msg)
            }
        })
        .interact_text()?;

    Ok(Task {
        id: task.id,
        task_id: task.task_id,
        timestamp: task.timestamp.clone(),
        name,
        comment,
        completeness: Some(completeness),
        excluded_from_search: task.excluded_from_search,
        tags: vec![], // Tags are preserved separately
    })
}

/// Creates a task from a named template.
///
/// Loads the specified template and allows the user to modify the template
/// values before creating the final task. This streamlines creation of
/// frequently used task types while maintaining flexibility.
///
/// # Arguments
///
/// * `template_name` - Name of the template to use for task creation
async fn handle_create_from_template(template_name: String) -> Result<()> {
    let mut templates_db = Templates::new()?;
    let template = match templates_db.get(&template_name)? {
        Some(t) => t,
        None => {
            msg_error!(Message::TemplateNotFound(template_name));
            return Ok(());
        }
    };

    msg_info!(Message::CreatingTaskFromTemplate(template.name.clone()));

    // Allow modification of template values
    let name = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTaskName.to_string())
        .default(template.task_name)
        .interact_text()?;

    let comment = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTaskComment.to_string())
        .default(template.comment)
        .allow_empty(true)
        .interact_text()?;

    let completeness = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTaskCompleteness.to_string())
        .default(template.completeness)
        .interact_text()?;

    // Create and display the new task
    let task = Task::new(&name, &comment, Some(completeness));
    let new_task = Tasks::new()?.insert(&task)?.update_id()?.get()?;
    View::tasks(&new_task)?;

    Ok(())
}

/// Interactive template selection for task creation.
///
/// Displays available templates in a selection interface, allowing users
/// to choose from existing templates without needing to remember template names.
async fn handle_create_from_template_interactive() -> Result<()> {
    let mut templates_db = Templates::new()?;
    let templates = templates_db.get_all()?;

    if templates.is_empty() {
        msg_info!(Message::NoTemplatesFound);
        msg_info!(Message::CreateTemplateFirst);
        return Ok(());
    }

    let template_options: Vec<String> = templates.iter().map(|t| format!("{} - {}", t.name, t.task_name)).collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::SelectTemplate.to_string())
        .items(&template_options)
        .interact()?;

    let template = &templates[selection];
    handle_create_from_template(template.name.clone()).await
}
