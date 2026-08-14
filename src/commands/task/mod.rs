//! Task management command.
//!
//! Provides comprehensive task management functionality for creating, editing, deleting, and organizing tasks.
//!
//! ## Usage
//!
//! ```bash
//! # Create a task interactively, or with values up front
//! kasl task
//! kasl task add --name "Review code" --comment "Check PR #123"
//!
//! # List tasks with different filters
//! kasl task list                      # Today's tasks
//! kasl task list --all                # Every task
//! kasl task list --tag urgent         # Tasks carrying a tag
//! kasl task show 42                   # Specific tasks by id
//!
//! # Edit and remove
//! kasl task edit 42                   # Edit one task
//! kasl task edit                      # Pick several interactively
//! kasl task remove 1 2 3
//! kasl task remove --today
//!
//! # Import from external services
//! kasl task find                      # Find tasks from GitLab/Jira
//! kasl task add --template "bug-fix"  # Create from template
//! ```

use crate::{
    db::tasks::Tasks,
    db::templates::Templates,
    libs::{
        messages::Message,
        pick,
        prompt::{ensure_interactive, is_interactive},
        stdin_drain::drain_available_stdin_lines,
        task::{Task, TaskFilter, collapse_whitespace},
        view::View,
    },
    msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use chrono::Local;
use clap::{Args, Subcommand};
use dialoguer::{Confirm, Input, theme::ColorfulTheme};

mod discovery;

/// Command-line arguments for task management.
///
/// Task operations are subcommands (`add`, `list`, `show`, `edit`, `remove`,
/// `find`). Running `kasl task` with no subcommand creates a task
/// interactively, which is the most frequent daily action.
#[derive(Debug, Args)]
pub struct TaskArgs {
    #[command(subcommand)]
    command: Option<TaskCommand>,
}

/// Available task operations.
#[derive(Debug, Subcommand)]
enum TaskCommand {
    /// Add a task
    #[command(about = "Add a task")]
    Add(AddArgs),

    /// List tasks
    #[command(about = "List tasks")]
    List(ListArgs),

    /// Show tasks by id
    #[command(about = "Show tasks by id")]
    Show {
        /// Task ids to show; omit to pick from today's tasks
        #[arg(value_name = "ID", num_args = 1..)]
        id: Vec<i32>,
    },

    /// Edit a task
    #[command(about = "Edit a task by id, or several interactively")]
    Edit {
        /// Task id to edit; omit to pick several interactively
        #[arg(value_name = "ID")]
        id: Option<i32>,
    },

    /// Remove tasks
    #[command(about = "Remove tasks by id, or all of today's")]
    Remove(RemoveArgs),

    /// Find incomplete and external tasks to import
    #[command(about = "Find incomplete tasks and import from GitLab/Jira")]
    Find,
}

/// Arguments for creating a task.
#[derive(Debug, Args, Default)]
pub struct AddArgs {
    /// Task name
    #[arg(short, long)]
    name: Option<String>,

    /// Task comment or description
    #[arg(long)]
    comment: Option<String>,

    /// Completion percentage (0-100)
    #[arg(short, long)]
    completeness: Option<i32>,

    /// Comma-separated tags to assign
    #[arg(long)]
    tags: Option<String>,

    /// Create from a named template
    #[arg(long, short = 't')]
    template: Option<String>,

    /// Pick a template interactively
    #[arg(long, short = 'l')]
    from_template: bool,
}

/// Arguments for listing tasks.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// List tasks from every date, not just today
    #[arg(short, long)]
    all: bool,

    /// Only tasks carrying this tag
    #[arg(long)]
    tag: Option<String>,
}

/// Arguments for removing tasks.
#[derive(Debug, Args)]
pub struct RemoveArgs {
    /// Task ids to remove
    #[arg(value_name = "ID", num_args = 1..)]
    id: Vec<i32>,

    /// Remove every task recorded for today
    #[arg(long)]
    today: bool,

    /// Remove without asking for confirmation
    #[arg(long, short = 'y')]
    yes: bool,
}

/// Dispatches `kasl task` subcommands; the module header shows the surface.
pub async fn cmd(task_args: TaskArgs) -> Result<()> {
    let date = Local::now();

    match task_args.command {
        Some(TaskCommand::Add(args)) => {
            // Template creation is a form of adding, so it lives here too.
            if let Some(template_name) = args.template {
                return handle_create_from_template(template_name).await;
            }
            if args.from_template {
                return handle_create_from_template_interactive().await;
            }
            handle_task_creation(args).await
        }
        Some(TaskCommand::List(args)) => {
            let filter = if args.all {
                TaskFilter::All
            } else if let Some(tag) = args.tag {
                TaskFilter::ByTag(tag)
            } else {
                TaskFilter::Date(date.date_naive())
            };
            show_tasks(filter)
        }
        Some(TaskCommand::Show { id }) => {
            let ids = if id.is_empty() {
                let today = Tasks::new()?.fetch(TaskFilter::Date(Local::now().date_naive()))?;
                pick::tasks(&today, "Show which tasks?")?
            } else {
                id
            };
            show_tasks(TaskFilter::ByIds(ids))
        }
        Some(TaskCommand::Edit { id }) => match id {
            Some(id) => handle_edit_by_id(id).await,
            None => handle_edit_interactive().await,
        },
        Some(TaskCommand::Remove(args)) => {
            if args.today {
                handle_delete_today(args.yes).await
            } else if args.id.is_empty() {
                msg_error!(Message::NoTaskIdsProvided);
                Ok(())
            } else {
                handle_delete_by_ids(args.id, args.yes).await
            }
        }
        Some(TaskCommand::Find) => discovery::handle_task_discovery(date).await,
        // Bare `kasl task` creates a task interactively - the daily entry point.
        None => handle_task_creation(AddArgs::default()).await,
    }
}

/// Fetches and renders tasks for the given filter.
fn show_tasks(filter: TaskFilter) -> Result<()> {
    let tasks = Tasks::new()?.fetch(filter)?;
    if tasks.is_empty() {
        msg_error!(Message::TaskNotFound);
        return Ok(());
    }
    View::tasks(&tasks)?;
    Ok(())
}

/// Prompts for a task name and absorbs leftover multi-line paste from stdin.
fn prompt_task_name_interactive() -> String {
    let raw = crate::libs::stdin_drain::read_pastable_line(&Message::PromptTaskName.to_string()).unwrap_or_default();
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
    !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_alphanumeric()) && !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

/// Handles manual task creation with interactive prompts.
async fn handle_task_creation(task_args: AddArgs) -> Result<()> {
    // Anything not supplied on the command line is asked for, so a missing name
    // means prompting - which must not happen with no one at the terminal.
    if task_args.name.is_none() {
        ensure_interactive("task name is required; pass --name outside an interactive terminal")?;
    }

    // Collect task information (from args or interactive prompts)
    let name_from_args = task_args.name.is_some();
    let comment_from_args = task_args.comment.is_some();

    let mut name = match task_args.name {
        Some(n) => collapse_whitespace(&n),
        None => prompt_task_name_interactive(),
    };

    // Only the name is required. With a name supplied but no comment or
    // completeness, the remaining prompts are skipped rather than attempted:
    // `task add --name X` has to work from a script, where there is nobody to
    // answer them.
    let interactive = is_interactive();

    let mut comment = match task_args.comment {
        Some(c) => collapse_whitespace(&c),
        None if !interactive => String::new(),
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
    if interactive && !name_from_args && !comment_from_args && looks_like_issue_key(&name) && !comment.is_empty() {
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

    let completeness = match task_args.completeness {
        Some(c) => c,
        None if !interactive => 100,
        None => Input::with_theme(&ColorfulTheme::default())
            .allow_empty(true)
            .with_prompt(Message::PromptTaskCompleteness.to_string())
            .default(100)
            .interact_text()
            .unwrap(),
    };

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
async fn handle_delete_by_ids(ids: Vec<i32>, assume_yes: bool) -> Result<()> {
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

    if !assume_yes {
        // Never block on a prompt when there is no one to answer it.
        ensure_interactive("refusing to remove tasks without --yes outside an interactive terminal")?;

        // Request confirmation based on number of tasks
        let prompt = if ids.len() == 1 {
            Message::ConfirmDeleteTask
        } else {
            Message::ConfirmDeleteTasks(ids.len())
        };

        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt.to_string())
            .default(false)
            .interact()?;

        if !confirmed {
            msg_info!(Message::OperationCancelled);
            return Ok(());
        }
    }

    let deleted_count = tasks_db.delete_many(&ids)?;
    msg_success!(Message::TasksDeletedCount(deleted_count));

    Ok(())
}

/// Handles deletion of all tasks for today.
///
/// This is a dangerous operation that removes all tasks created today.
/// It includes multiple confirmation steps and detailed previews to
/// prevent accidental data loss.
async fn handle_delete_today(assume_yes: bool) -> Result<()> {
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

    if !assume_yes {
        // Never block on a prompt when there is no one to answer it.
        ensure_interactive("refusing to remove today's tasks without --yes outside an interactive terminal")?;

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

        if !second_confirm {
            msg_info!(Message::OperationCancelled);
            return Ok(());
        }
    }

    let ids: Vec<i32> = tasks.iter().filter_map(|t| t.id).collect();
    let deleted_count = tasks_db.delete_many(&ids)?;
    msg_success!(Message::TasksDeletedCount(deleted_count));

    Ok(())
}

/// Handles editing a single task by its ID.
///
/// Provides an interactive editing interface for modifying task properties
/// including name, comment, and completion status. Includes preview of
/// changes before applying them to the database.
async fn handle_edit_by_id(id: i32) -> Result<()> {
    // Editing prompts for each field with the current value as default.
    ensure_interactive("`kasl task edit` is interactive and needs a terminal")?;

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
    View::tasks(std::slice::from_ref(&task))?;

    // Interactive editing
    let edited_task = edit_task_interactive(&task)?;

    // Check if anything actually changed
    if edited_task.name == task.name && edited_task.comment == task.comment && edited_task.completeness == task.completeness {
        msg_info!(Message::NoChangesDetected);
        return Ok(());
    }

    // Show preview of changes
    msg_print!(Message::TaskEditPreview, true);
    View::tasks(std::slice::from_ref(&edited_task))?;

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
    // Picks tasks from a MultiSelect, then prompts per task.
    ensure_interactive("`kasl task edit` without an id is interactive and needs a terminal")?;

    let mut tasks_db = Tasks::new()?;

    // Get today's tasks for selection
    let today = Local::now().date_naive();
    let tasks = tasks_db.fetch(TaskFilter::Date(today))?;

    if tasks.is_empty() {
        msg_info!(Message::NoTasksForToday);
        return Ok(());
    }

    let ids = pick::tasks(&tasks, &Message::SelectTasksToEdit.to_string())?;

    if ids.is_empty() {
        msg_info!(Message::NoTasksSelected);
        return Ok(());
    }

    // Edit each selected task in sequence
    for task in tasks.iter().filter(|t| t.id.is_some_and(|id| ids.contains(&id))) {
        msg_print!(Message::EditingTask(task.name.clone()), true);
        View::tasks(std::slice::from_ref(task))?;

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
            if *input >= 0 && *input <= 100 { Ok(()) } else { Err(&completeness_range_msg) }
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
    // Template is chosen from a Select.
    ensure_interactive("`--from-template` is interactive; pass --template NAME outside a terminal")?;

    let mut templates_db = Templates::new()?;
    let templates = templates_db.get_all()?;

    if templates.is_empty() {
        msg_info!(Message::NoTemplatesFound);
        msg_info!(Message::CreateTemplateFirst);
        return Ok(());
    }

    let name = pick::template(&templates, &Message::SelectTemplate.to_string())?;
    handle_create_from_template(name).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libs::config::default_ignore_names;
    use crate::libs::task::{is_ignored_name, normalize_task_name};

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
        assert_eq!(collapse_whitespace(pasted), "PROJ-42 Fix login redirect for OAuth callback");
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

        assert!(is_ignored_name("Merge remote-tracking branch 'origin/release/4.39.0' into feature/x", &ignore));
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
}
