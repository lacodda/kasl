//! Contains the logic for the `task` command.
//!
//! This command allows users to create, view, and find tasks. It can
//! aggregate tasks from multiple sources, including incomplete local tasks
//! and external services like GitLab and Jira.

use crate::{
    api::{gitlab::GitLab, jira::Jira},
    db::tasks::Tasks,
    libs::{
        config::Config,
        messages::Message,
        task::{Task, TaskFilter},
        view::View,
    },
    msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use chrono::Local;
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect};

/// Enum to identify the origin of a task suggestion.
#[derive(Debug, PartialEq, Eq, Hash)]
enum TaskSource {
    Incomplete,
    Gitlab,
    Jira,
}

/// Command-line arguments for the `task` command.
#[derive(Debug, Args)]
pub struct TaskArgs {
    /// The name of the task to create.
    #[arg(short, long)]
    name: Option<String>,
    /// An optional comment for the task.
    #[arg(long)]
    comment: Option<String>,
    /// The completeness percentage of the task (e.g., 100).
    #[arg(short, long)]
    completeness: Option<i32>,
    /// Show existing tasks.
    #[arg(short, long)]
    show: bool,
    /// Used with `--show` to display all tasks, not just today's.
    #[arg(short, long)]
    all: bool,
    /// Used with `--show` to display tasks by specific IDs.
    #[arg(short, long)]
    id: Option<Vec<i32>>,
    /// Find and suggest tasks from various sources (incomplete, GitLab, Jira).
    #[arg(short, long, help = "Find incomplete tasks")]
    find: bool,
    /// Delete tasks by IDs
    #[arg(short, long, help = "Delete tasks by IDs")]
    delete: Option<Vec<i32>>,
    /// Delete all tasks for today (use with caution)
    #[arg(long, help = "Delete all tasks for today")]
    delete_today: bool,
    /// Edit task by ID
    #[arg(short, long, help = "Edit task by ID")]
    edit: Option<i32>,
    /// Edit multiple tasks interactively
    #[arg(long, help = "Edit multiple tasks interactively")]
    edit_interactive: bool,
}

/// Main entry point for the `task` command.
///
/// This function is a large dispatcher that handles different actions based on
/// the provided flags:
/// - `--show`: Displays tasks based on filters (`--all`, `--id`, or default to today).
/// - `--find`: Aggregates tasks from different sources and presents an interactive selection.
/// - Default: Enters an interactive mode to create a new task.
pub async fn cmd(task_args: TaskArgs) -> Result<()> {
    let date = Local::now();

    // Handle deletion
    if let Some(ids) = task_args.delete {
        return handle_delete_by_ids(ids).await;
    }

    if task_args.delete_today {
        return handle_delete_today().await;
    }

    // Handle editing
    if let Some(id) = task_args.edit {
        return handle_edit_by_id(id).await;
    }

    if task_args.edit_interactive {
        return handle_edit_interactive().await;
    }

    // Handle showing tasks
    if task_args.show {
        let mut filter: TaskFilter = TaskFilter::Date(date.date_naive());
        if task_args.all {
            filter = TaskFilter::All;
        } else if task_args.id.is_some() {
            filter = TaskFilter::ByIds(task_args.id.unwrap());
        }
        let tasks = Tasks::new()?.fetch(filter)?;
        if tasks.is_empty() {
            msg_error!(Message::TaskNotFound);
            return Ok(());
        }
        View::tasks(&tasks)?;

        return Ok(());
    // Handle finding tasks from multiple sources
    } else if task_args.find {
        // Incomplete tasks
        let mut tasks: Vec<(&TaskSource, Vec<Task>)> = Vec::new();
        let incomplete_tasks = Tasks::new()?.fetch(TaskFilter::Incomplete)?;

        if !incomplete_tasks.is_empty() {
            tasks.push((&TaskSource::Incomplete, incomplete_tasks));
        }

        let config = Config::read()?;
        // GitLab commits
        if config.gitlab.is_some() {
            // The `get_today_commits` function is designed to be resilient.
            // It returns an empty Vec on network or parsing errors instead of panicking,
            // logging the error to stderr internally.
            let today_tasks = Tasks::new()?.fetch(TaskFilter::Date(date.date_naive()))?;
            let commits = GitLab::new(&config.gitlab.unwrap()).get_today_commits().await.unwrap_or_default();

            let mut gitlab_tasks: Vec<Task> = Vec::new();
            commits.iter().for_each(|commit| {
                if today_tasks.iter().all(|task| task.name != commit.message) {
                    gitlab_tasks.push(Task::new(&commit.message, "", Some(100)));
                }
            });
            if !gitlab_tasks.is_empty() {
                tasks.push((&TaskSource::Gitlab, gitlab_tasks));
            }
        }
        // Jira issues
        if config.jira.is_some() {
            // The `get_completed_issues` function is also resilient and returns
            // an empty Vec on error, preventing the command from crashing.
            let jira_issues = Jira::new(&config.jira.unwrap()).get_completed_issues(&date.date_naive()).await?;
            let mut jira_tasks: Vec<Task> = Vec::new();
            jira_issues.iter().for_each(|issue| {
                let name = format!("{} {}", &issue.key, &issue.fields.summary);
                jira_tasks.push(Task::new(&name, "", Some(100)));
            });
            if !jira_tasks.is_empty() {
                tasks.push((&TaskSource::Jira, jira_tasks));
            }
        }

        if tasks.iter().all(|(_, task)| task.is_empty()) {
            msg_error!(Message::TasksNotFoundSad);
            return Ok(());
        }

        let mut selected_tasks: Vec<(&TaskSource, Vec<usize>)> = Vec::new();
        for (task_source, tasks) in tasks.iter() {
            let mut name_format: Box<dyn Fn(&Task) -> String> = Box::new(|task: &Task| task.name.to_owned());
            match task_source {
                TaskSource::Incomplete => {
                    msg_print!(Message::TasksIncompleteHeader, true);
                    name_format = Box::new(|task: &Task| format!("{} - {}%", task.name, task.completeness.unwrap_or(0)));
                }
                TaskSource::Gitlab => msg_print!(Message::TasksGitlabHeader, true),
                TaskSource::Jira => msg_print!(Message::TasksJiraHeader, true),
            }
            let task_names: Vec<String> = tasks.iter().map(name_format).collect();
            selected_tasks.push((
                task_source,
                MultiSelect::with_theme(&ColorfulTheme::default())
                    .with_prompt(Message::PromptSelectOptions.to_string())
                    .items(&task_names)
                    .interact()
                    .unwrap(),
            ));
        }

        for (task_source, selected_task_indexes) in selected_tasks {
            for index in selected_task_indexes {
                let mut task = tasks.iter().find(|(ts, _)| ts == &task_source).map_or(&vec![], |(_, tasks)| tasks)[index].clone();
                match task_source {
                    TaskSource::Incomplete => {
                        msg_print!(Message::SelectingTask(task.name.clone()));
                        if task.task_id.is_none() || task.task_id.is_some_and(|id| id == 0) {
                            task.task_id = task.id;
                        }
                        let default_completeness = (task.completeness.unwrap() + 1).min(100);
                        task.completeness = Some(
                            Input::with_theme(&ColorfulTheme::default())
                                .allow_empty(true)
                                .with_prompt(Message::PromptTaskCompleteness.to_string())
                                .default(default_completeness)
                                .interact_text()
                                .unwrap(),
                        );
                    }
                    _ => {}
                }
                let _ = Tasks::new()?.insert(&task);
            }
        }

        return Ok(());
    }

    // Handle creating a new task
    let name = task_args.name.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptTaskName.to_string())
            .interact_text()
            .unwrap()
    });
    let comment = task_args.comment.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .allow_empty(true)
            .with_prompt(Message::PromptTaskComment.to_string())
            .interact_text()
            .unwrap()
    });
    let completeness = task_args.completeness.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .allow_empty(true)
            .with_prompt(Message::PromptTaskCompleteness.to_string())
            .default(100)
            .interact_text()
            .unwrap()
    });

    let task = Task::new(&name, &comment, Some(completeness));
    let new_task = Tasks::new()?.insert(&task)?.update_id()?.get()?;
    View::tasks(&new_task)?;

    Ok(())
}

/// Handle deletion of tasks by IDs
async fn handle_delete_by_ids(ids: Vec<i32>) -> Result<()> {
    if ids.is_empty() {
        msg_error!(Message::NoTaskIdsProvided);
        return Ok(());
    }

    let mut tasks_db = Tasks::new()?;

    // Fetch tasks to show what will be deleted
    let tasks = tasks_db.fetch(TaskFilter::ByIds(ids.clone()))?;

    if tasks.is_empty() {
        msg_error!(Message::TasksNotFoundForIds(ids));
        return Ok(());
    }

    // Show tasks that will be deleted
    msg_print!(Message::TasksToBeDeleted, true);
    View::tasks(&tasks)?;

    // Confirm deletion
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

/// Handle deletion of all tasks for today
async fn handle_delete_today() -> Result<()> {
    let mut tasks_db = Tasks::new()?;
    let today = Local::now().date_naive();

    // Fetch today's tasks
    let tasks = tasks_db.fetch(TaskFilter::Date(today))?;

    if tasks.is_empty() {
        msg_info!(Message::NoTasksForToday);
        return Ok(());
    }

    // Show tasks that will be deleted
    msg_print!(Message::TasksToBeDeleted, true);
    View::tasks(&tasks)?;

    // Double confirmation for bulk delete
    let first_confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::ConfirmDeleteAllTodayTasks(tasks.len()).to_string())
        .default(false)
        .interact()?;

    if !first_confirm {
        msg_info!(Message::OperationCancelled);
        return Ok(());
    }

    // Second confirmation
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

/// Handle editing a single task by ID
async fn handle_edit_by_id(id: i32) -> Result<()> {
    let mut tasks_db = Tasks::new()?;

    // Fetch the task
    let task = match tasks_db.get_by_id(id)? {
        Some(task) => task,
        None => {
            msg_error!(Message::TaskNotFoundWithId(id));
            return Ok(());
        }
    };

    // Show current task
    msg_print!(Message::CurrentTaskState, true);
    View::tasks(&[task.clone()])?;

    // Interactive edit
    let edited_task = edit_task_interactive(&task)?;

    // Check if anything changed
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

/// Handle interactive editing of multiple tasks
async fn handle_edit_interactive() -> Result<()> {
    let mut tasks_db = Tasks::new()?;

    // Show tasks for today by default
    let today = Local::now().date_naive();
    let tasks = tasks_db.fetch(TaskFilter::Date(today))?;

    if tasks.is_empty() {
        msg_info!(Message::NoTasksForToday);
        return Ok(());
    }

    // Let user select which tasks to edit
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

    // Edit each selected task
    for &index in &selections {
        let task = &tasks[index];

        msg_print!(Message::EditingTask(task.name.clone()), true);
        View::tasks(&[task.clone()])?;

        let edited_task = edit_task_interactive(task)?;

        // Update if changed
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

/// Interactive task editing helper
fn edit_task_interactive(task: &Task) -> Result<Task> {
    let name = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTaskNameEdit.to_string())
        .default(task.name.clone())
        .interact_text()?;

    let comment = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTaskCommentEdit.to_string())
        .default(task.comment.clone())
        .allow_empty(true)
        .interact_text()?;

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
    })
}
