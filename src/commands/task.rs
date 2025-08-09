//! Comprehensive task management command.
//!
//! This module provides the core task management functionality of kasl, enabling users
//! to create, edit, delete, and organize tasks. It integrates with external services
//! like GitLab and Jira to automatically import work items, and supports advanced
//! features like templates, tags, and batch operations.
//!
//! ## Task Sources
//!
//! Tasks can be created from multiple sources:
//! - **Manual Creation**: Direct user input with name, comment, and completion
//! - **GitLab Integration**: Automatic import of daily commits as completed tasks
//! - **Jira Integration**: Import of completed issues and work items
//! - **Template System**: Quick creation from predefined task templates
//! - **Incomplete Tasks**: Continuation of previously started work
//!
//! ## Task Operations
//!
//! - **CRUD Operations**: Create, read, update, and delete individual tasks
//! - **Batch Operations**: Mass editing and deletion of multiple tasks
//! - **Filtering**: View tasks by date, completion status, tags, or IDs
//! - **Search Integration**: Find and update incomplete tasks across date ranges
//! - **Tag Management**: Organize tasks with custom categorization

use crate::{
    api::{gitlab::GitLab, jira::Jira},
    db::tasks::Tasks,
    db::templates::Templates,
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
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};

/// Enumeration for identifying task suggestion sources.
///
/// This enum helps distinguish between different sources of task suggestions
/// during the interactive task finding process, allowing for appropriate
/// handling and user feedback for each source type.
#[derive(Debug, PartialEq, Eq, Hash)]
enum TaskSource {
    /// Previously created but incomplete local tasks
    Incomplete,
    /// Commits from GitLab repositories for the current day
    Gitlab,
    /// Completed issues from Jira for the current day
    Jira,
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
/// This function implements the sophisticated task discovery system that aggregates
/// potential tasks from various sources and presents them in a unified interface
/// for selection and import.
///
/// ## Discovery Sources
///
/// 1. **Incomplete Local Tasks**: Tasks with completion < 100% that haven't been
///    worked on today, allowing users to continue previous work
/// 2. **GitLab Commits**: Today's commits from configured repositories, imported
///    as completed tasks with commit messages as task names
/// 3. **Jira Issues**: Completed issues from today, imported with issue keys
///    and summaries as task descriptions
///
/// ## Selection Interface
///
/// For each source with available tasks, the function:
/// - Displays a categorized list with appropriate headers
/// - Allows multi-selection of desired tasks
/// - Shows task-specific information (completion %, commit messages, etc.)
/// - Handles source-specific processing for selected items
///
/// ## Error Resilience
///
/// External API integrations are designed to be resilient:
/// - Network errors don't crash the application
/// - API failures are logged but don't prevent local task discovery
/// - Missing configurations gracefully skip external sources
/// - Users can still work with local tasks even if external services are down
///
/// # Arguments
///
/// * `date` - Current date/time for filtering today's external content
async fn handle_task_discovery(date: chrono::DateTime<Local>) -> Result<()> {
    let mut tasks: Vec<(&TaskSource, Vec<Task>)> = Vec::new();

    // Discover incomplete local tasks
    let incomplete_tasks = Tasks::new()?.fetch(TaskFilter::Incomplete)?;
    if !incomplete_tasks.is_empty() {
        tasks.push((&TaskSource::Incomplete, incomplete_tasks));
    }

    let config = Config::read()?;

    // Discover GitLab commits if configured
    if config.gitlab.is_some() {
        // Get existing tasks to avoid duplicates
        let today_tasks = Tasks::new()?.fetch(TaskFilter::Date(date.date_naive()))?;
        let commits = GitLab::new(&config.gitlab.unwrap()).get_today_commits().await.unwrap_or_default(); // Graceful fallback on error

        let mut gitlab_tasks: Vec<Task> = Vec::new();
        commits.iter().for_each(|commit| {
            // Only add commits that aren't already tasks
            if today_tasks.iter().all(|task| task.name != commit.message) {
                gitlab_tasks.push(Task::new(&commit.message, "", Some(100)));
            }
        });

        if !gitlab_tasks.is_empty() {
            tasks.push((&TaskSource::Gitlab, gitlab_tasks));
        }
    }

    // Discover Jira issues if configured
    if config.jira.is_some() {
        let jira_issues = Jira::new(&config.jira.unwrap()).get_completed_issues(&date.date_naive()).await?; // Note: Jira errors are propagated unlike GitLab

        let mut jira_tasks: Vec<Task> = Vec::new();
        jira_issues.iter().for_each(|issue| {
            let name = format!("{} {}", &issue.key, &issue.fields.summary);
            jira_tasks.push(Task::new(&name, "", Some(100)));
        });

        if !jira_tasks.is_empty() {
            tasks.push((&TaskSource::Jira, jira_tasks));
        }
    }

    // Check if any tasks were discovered
    if tasks.iter().all(|(_, task)| task.is_empty()) {
        msg_error!(Message::TasksNotFoundSad);
        return Ok(());
    }

    // Present selection interface for each source
    let mut selected_tasks: Vec<(&TaskSource, Vec<usize>)> = Vec::new();
    for (task_source, tasks) in tasks.iter() {
        // Configure display format based on source type
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

    // Process selected tasks based on their source
    for (task_source, selected_task_indexes) in selected_tasks {
        for index in selected_task_indexes {
            let mut task = tasks.iter().find(|(ts, _)| ts == &task_source).map_or(&vec![], |(_, tasks)| tasks)[index].clone();

            match task_source {
                TaskSource::Incomplete => {
                    // Handle incomplete task continuation
                    msg_print!(Message::SelectingTask(task.name.clone()));

                    // Ensure task_id is properly set for continuation
                    if task.task_id.is_none() || task.task_id.is_some_and(|id| id == 0) {
                        task.task_id = task.id;
                    }

                    // Prompt for updated completion percentage
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
                _ => {
                    // GitLab and Jira tasks are imported as-is (already complete)
                }
            }

            // Insert the task into the database
            let _ = Tasks::new()?.insert(&task);
        }
    }

    Ok(())
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
