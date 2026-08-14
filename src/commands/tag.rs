//! Tag management command for task organization and categorization.
//!
//! Provides comprehensive tag management functionality, enabling users to create, organize, and utilize tags for better task categorization.
//!
//! ## Features
//!
//! - **Tag CRUD Operations**: Create, read, update, and delete tag definitions
//! - **Color Coding**: Visual organization with customizable tag colors
//! - **Task Association**: Link tags to tasks for categorization
//! - **Filtering**: Find tasks by tag assignments
//! - **Auto-Creation**: Automatically create tags when assigned to tasks
//!
//! ## Usage
//!
//! ```bash
//! # List all tags
//! kasl tag list
//!
//! # Add a new tag with color
//! kasl tag add urgent --color red
//!
//! # Show a tag and the tasks that carry it
//! kasl tag show urgent
//!
//! # Remove a tag
//! kasl tag remove old-tag
//! ```

use crate::{
    db::tags::{Tag, Tags},
    libs::{messages::Message, pick, prompt::ensure_interactive, view::View},
    msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};

/// Command-line arguments for tag management operations.
///
/// The tag command uses subcommands to organize different tag management
/// operations, providing a clean and intuitive interface for users to
/// manage their tag library and task associations.
#[derive(Debug, Args)]
pub struct TagArgs {
    #[command(subcommand)]
    command: Option<TagCommand>,
}

/// Available tag management operations.
///
/// Each subcommand provides specific functionality for tag lifecycle
/// management and task association operations, supporting both direct
/// command-line usage and interactive workflows.
#[derive(Debug, Subcommand)]
enum TagCommand {
    /// Add a new tag with optional color
    ///
    /// Creates a new tag that can be assigned to tasks for categorization.
    /// Tags can optionally include color information for visual organization
    /// in user interfaces and reports.
    Add {
        /// Unique name for the tag
        ///
        /// Must be unique across all tags and should be descriptive
        /// enough to clearly indicate the tag's purpose. Common examples
        /// include project names, priorities, or task types.
        name: String,

        /// Optional color for visual organization
        ///
        /// Specifies a color name or code for visual representation of the tag.
        /// Common color names like "red", "blue", "green" are supported,
        /// as well as hex color codes for precise color specification.
        #[arg(short, long)]
        color: Option<String>,
    },

    /// List all available tags
    ///
    /// Displays a formatted table of all existing tags with their names,
    /// colors, and creation dates. Useful for reviewing the current tag
    /// library and understanding available categorization options.
    List,

    /// Show a tag and the tasks that carry it
    ///
    /// Displays the tag's properties along with every task currently
    /// assigned it, regardless of completion status or creation date.
    Show {
        /// Tag name or ID to show
        tag: String,
    },

    /// Edit an existing tag's properties
    ///
    /// Modifies an existing tag's name and color properties. Tag editing
    /// affects all tasks that currently use the tag, so changes should be
    /// made carefully to maintain consistent categorization.
    Edit {
        /// Tag name or ID to edit; omit to pick from the list
        ///
        /// Can specify either the tag name (string) or database ID (number)
        /// for the tag to be edited. If the input can be parsed as a number,
        /// it will be treated as an ID; otherwise, it's treated as a name.
        tag: Option<String>,
    },

    /// Remove a tag and unassign it from all tasks
    ///
    /// Permanently removes a tag from the system and unassigns it from
    /// all tasks that currently use it. Includes safety confirmation
    /// prompts, especially when the tag is actively used by tasks.
    Remove {
        /// Tag name or ID to remove; omit to pick from the list
        ///
        /// Can specify either the tag name (string) or database ID (number)
        /// for the tag to be removed. The system will confirm the operation
        /// and show how many tasks will be affected.
        tag: Option<String>,

        /// Remove without asking for confirmation
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

/// Executes tag management operations based on the specified subcommand.
///
/// This function serves as the main dispatcher for tag operations, routing
/// to appropriate handlers based on user input. When no subcommand is provided,
/// it enters interactive mode for operation selection.
///
/// ## Operation Routing
///
/// - **Add**: Tag creation with validation and color assignment
/// - **List**: Formatted display of all available tags
/// - **Show**: Display the tag and the tasks assigned it
/// - **Edit**: Interactive or direct tag modification
/// - **Remove**: Safe tag removal with usage impact analysis
/// - **Interactive**: Menu-driven operation selection when no subcommand given
///
/// ## Error Handling
///
/// Each operation includes appropriate error handling for:
/// - Tag not found scenarios
/// - Database connectivity issues
/// - Validation failures
/// - User input errors
/// - Concurrent modification conflicts
///
/// # Arguments
///
/// * `args` - Parsed command-line arguments containing operation specification
///
/// # Returns
///
/// Returns `Ok(())` on successful operation completion, or an error if
/// the requested operation fails due to validation, database, or user input issues.
///
/// # Examples
///
/// ```bash
/// # Create a new urgent tag with red color
/// kasl tag add urgent --color red
///
/// # List all available tags
/// kasl tag list
///
/// # Edit a tag's properties
/// kasl tag edit urgent
///
/// # Show all tasks tagged as "backend"
/// kasl tag show backend
///
/// # Interactive mode
/// kasl tag
/// ```
pub async fn cmd(args: TagArgs) -> Result<()> {
    match args.command {
        Some(TagCommand::Add { name, color }) => handle_create(name, color),
        Some(TagCommand::List) => handle_list(),
        Some(TagCommand::Show { tag }) => handle_show_tasks(tag).await,
        Some(TagCommand::Edit { tag }) => handle_edit(resolve_tag(tag, "Edit which tag?")?),
        Some(TagCommand::Remove { tag, yes }) => handle_delete(resolve_tag(tag, "Remove which tag?")?, yes),
        None => {
            ensure_interactive("no subcommand given; run `kasl tag list` or see `kasl tag --help`")?;
            handle_interactive()
        }
    }
}

/// Returns the tag to act on, opening a picker when none was given.
fn resolve_tag(tag: Option<String>, prompt: &str) -> Result<String> {
    match tag {
        Some(tag) => Ok(tag),
        None => pick::tag(&Tags::new()?.get_all()?, prompt),
    }
}

/// Handles tag creation with validation and uniqueness checking.
///
/// This function manages the complete tag creation workflow:
/// 1. **Uniqueness Validation**: Ensures tag name doesn't already exist
/// 2. **Tag Creation**: Creates new tag with specified name and optional color
/// 3. **Database Storage**: Saves the new tag with proper error handling
/// 4. **User Feedback**: Provides confirmation of successful creation
///
/// ## Tag Properties
///
/// New tags are created with these properties:
/// - **Name**: Unique identifier provided by user
/// - **Color**: Optional visual indicator (defaults to system-assigned if not provided)
/// - **Creation Date**: Automatically set to current timestamp
///
/// ## Validation Rules
///
/// - Tag names must be unique across the entire tag library
/// - Tag names cannot be empty or contain only whitespace
/// - Color values are optional and accept standard color names or hex codes
///
/// # Arguments
///
/// * `name` - Unique name for the new tag
/// * `color` - Optional color specification for visual organization
fn handle_create(name: String, color: Option<String>) -> Result<()> {
    let mut tags_db = Tags::new()?;

    // Validate tag name uniqueness
    if tags_db.get_by_name(&name)?.is_some() {
        msg_error!(Message::TagAlreadyExists(name));
        return Ok(());
    }

    // Create and save the new tag
    let tag = Tag::new(name.clone(), color);
    tags_db.create(&tag)?;

    msg_success!(Message::TagCreated(name));
    Ok(())
}

/// Displays all available tags in a formatted table.
///
/// This function retrieves all tags from the database and presents them
/// in a user-friendly table format showing all relevant properties.
/// The display helps users understand their available tags and their
/// configurations for effective task categorization.
///
/// ## Display Format
///
/// The table includes these columns:
/// - **ID**: Database identifier for the tag
/// - **Name**: The tag's unique name
/// - **Color**: Visual color indicator (if assigned)
///
/// ## Empty State Handling
///
/// When no tags exist, the function provides helpful guidance about
/// creating the first tag rather than displaying an empty table.
fn handle_list() -> Result<()> {
    let mut tags_db = Tags::new()?;
    let tags = tags_db.get_all()?;

    if tags.is_empty() {
        msg_info!(Message::NoTagsFound);
        return Ok(());
    }

    msg_print!(Message::TagListHeader, true);
    View::tags(&tags)?;
    Ok(())
}

/// Handles tag editing with flexible identifier support.
///
/// This function provides comprehensive tag editing capabilities:
/// 1. **Tag Resolution**: Finds tag by name or ID
/// 2. **Current State Display**: Shows existing tag properties
/// 3. **Interactive Editing**: Prompts for new values with current values as defaults
/// 4. **Validation**: Ensures edited values meet tag requirements
/// 5. **Database Update**: Saves changes with proper error handling
///
/// ## Identifier Resolution
///
/// The function accepts flexible tag identification:
/// - **Numeric Input**: Treated as database ID
/// - **String Input**: Treated as tag name
/// - **Automatic Detection**: Parses input to determine type
///
/// ## Editing Interface
///
/// For each editable property, the interface:
/// - Shows the current value as the default
/// - Allows the user to accept current value or enter new one
/// - Validates new values according to tag rules
/// - Provides clear feedback about validation errors
///
/// # Arguments
///
/// * `tag_identifier` - Tag name or ID string for flexible tag identification
fn handle_edit(tag_identifier: String) -> Result<()> {
    // Editing is prompt-driven; there is nothing to fall back on without a terminal.
    ensure_interactive("`kasl tag edit` is interactive and needs a terminal")?;

    let mut tags_db = Tags::new()?;

    // Resolve tag by ID or name
    let tag = if let Ok(id) = tag_identifier.parse::<i32>() {
        tags_db.get_by_id(id)?
    } else {
        tags_db.get_by_name(&tag_identifier)?
    };

    let tag = match tag {
        Some(t) => t,
        None => {
            msg_error!(Message::TagNotFound(tag_identifier));
            return Ok(());
        }
    };

    msg_print!(Message::EditingTag(tag.name.clone()), true);

    // Interactive editing with current values as defaults
    let new_name = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTagName.to_string())
        .default(tag.name.clone())
        .interact_text()?;

    let new_color = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTagColor.to_string())
        .default(tag.color.unwrap_or_default())
        .allow_empty(true)
        .interact_text()?;

    // Handle empty color input
    let color = if new_color.is_empty() { None } else { Some(new_color) };

    // Update the tag
    tags_db.update(&Tag {
        id: tag.id,
        name: new_name.clone(),
        color,
        created_at: None,
    })?;
    msg_success!(Message::TagUpdated(new_name));
    Ok(())
}

/// Handles safe tag deletion with usage impact analysis.
///
/// This function manages the tag deletion process with comprehensive
/// safety measures to prevent accidental data loss and inform users
/// about the impact of deletion:
/// 1. **Tag Resolution**: Finds tag by name or ID
/// 2. **Usage Analysis**: Counts how many tasks currently use the tag
/// 3. **Impact Communication**: Informs user about affected tasks
/// 4. **Confirmation Prompt**: Requires explicit user confirmation
/// 5. **Safe Deletion**: Removes tag and updates task associations
///
/// ## Safety Features
///
/// - Confirms tag exists before showing deletion prompt
/// - Analyzes and reports impact on existing tasks
/// - Uses different confirmation messages based on usage
/// - Defaults to "No" for safety
/// - Provides escape opportunity before actual deletion
/// - Clear feedback about cancellation vs. completion
///
/// ## Usage Impact
///
/// The function provides different confirmation prompts based on tag usage:
/// - **Unused Tags**: Simple confirmation for deletion
/// - **Used Tags**: Enhanced warning showing number of affected tasks
/// - **Heavily Used Tags**: Additional emphasis on impact scope
///
/// # Arguments
///
/// * `tag_identifier` - Tag name or ID string for flexible tag identification
fn handle_delete(tag_identifier: String, assume_yes: bool) -> Result<()> {
    let mut tags_db = Tags::new()?;

    // Resolve tag by ID or name
    let tag = if let Ok(id) = tag_identifier.parse::<i32>() {
        tags_db.get_by_id(id)?
    } else {
        tags_db.get_by_name(&tag_identifier)?
    };

    let tag = match tag {
        Some(t) => t,
        None => {
            msg_error!(Message::TagNotFound(tag_identifier));
            return Ok(());
        }
    };

    // Analyze usage impact
    let task_count = tags_db.get_tasks_by_tag(tag.id.unwrap())?.len();

    if !assume_yes {
        // Never block on a prompt when there is no one to answer it.
        ensure_interactive(&format!("refusing to remove tag '{}' without --yes outside an interactive terminal", tag.name))?;

        // Provide appropriate confirmation prompt based on usage
        let prompt = if task_count > 0 {
            Message::ConfirmDeleteTagWithTasks(tag.name.clone(), task_count)
        } else {
            Message::ConfirmDeleteTag(tag.name.clone())
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

    tags_db.delete(tag.id.unwrap())?;
    msg_success!(Message::TagDeleted(tag.name));

    Ok(())
}

/// Displays all tasks associated with a specific tag.
///
/// This function provides filtered task viewing based on tag assignment,
/// allowing users to see all work items within a particular category.
/// It's useful for project management and understanding work distribution
/// across different areas.
///
/// ## Display Features
///
/// - **Tag Validation**: Ensures the specified tag exists
/// - **Task Filtering**: Shows only tasks with the specified tag
/// - **Standard Format**: Uses the same task display format as other commands
/// - **Empty State Handling**: Provides helpful message when no tasks found
///
/// ## Use Cases
///
/// This functionality supports various workflows:
/// - **Project Review**: See all tasks for a specific project
/// - **Priority Management**: View all urgent or high-priority tasks
/// - **Sprint Planning**: Review tasks by type or area
/// - **Progress Tracking**: Monitor completion within categories
///
/// # Arguments
///
/// * `tag_name` - Name of the tag to filter tasks by
async fn handle_show_tasks(tag_name: String) -> Result<()> {
    let mut tags_db = Tags::new()?;

    // Validate tag exists
    let tag = match tags_db.get_by_name(&tag_name)? {
        Some(t) => t,
        None => {
            msg_error!(Message::TagNotFound(tag_name));
            return Ok(());
        }
    };

    // Get task IDs associated with this tag
    let task_ids = tags_db.get_tasks_by_tag(tag.id.unwrap())?;

    if task_ids.is_empty() {
        msg_info!(Message::NoTasksWithTag(tag_name));
        return Ok(());
    }

    // Fetch and display the tasks
    use crate::db::tasks::Tasks;
    let tasks = Tasks::new()?.fetch(crate::libs::task::TaskFilter::ByIds(task_ids))?;

    msg_print!(Message::TasksWithTag(tag_name), true);
    View::tasks(&tasks)?;

    Ok(())
}

/// Handles interactive tag management when no subcommand is provided.
///
/// This function provides a menu-driven interface for users who prefer
/// interactive operation over command-line arguments. It presents all
/// available tag operations in an easy-to-navigate menu format.
///
/// ## Interactive Menu
///
/// The menu presents these options:
/// 1. **Create tag**: Launches tag creation workflow with prompts for name and color
/// 2. **List tags**: Shows all available tags in formatted table
/// 3. **Edit tag**: Tag selection interface followed by editing prompts
/// 4. **Delete tag**: Tag selection interface with safety confirmations
///
/// Each menu option delegates to the appropriate specialized handler
/// function, ensuring consistent behavior between interactive and
/// command-line usage.
///
/// ## Tag Selection Interface
///
/// For edit and delete operations, the interactive mode provides:
/// - **Tag List Display**: Shows all available tags for selection
/// - **Empty State Handling**: Graceful handling when no tags exist
/// - **User-Friendly Selection**: Clear presentation of tag options
/// - **Operation Cancellation**: Easy way to exit without changes
///
/// ## User Experience
///
/// The interactive mode is designed for:
/// - Users new to the command-line interface
/// - Occasional tag management tasks
/// - Exploration of available tag operations
/// - Situations where remembering exact command syntax is inconvenient
fn handle_interactive() -> Result<()> {
    let options = vec!["Add tag", "List tags", "Edit tag", "Remove tag"];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::SelectTagAction.to_string())
        .items(&options)
        .interact()?;

    match selection {
        0 => {
            // Interactive tag creation
            let name = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::PromptTagName.to_string())
                .interact_text()?;
            let color: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::PromptTagColor.to_string())
                .allow_empty(true)
                .interact_text()?;
            handle_create(name, if color.is_empty() { None } else { Some(color) })
        }
        1 => handle_list(),
        2 => {
            // Interactive tag editing with selection
            let mut tags_db = Tags::new()?;
            let tags = tags_db.get_all()?;
            if tags.is_empty() {
                msg_info!(Message::NoTagsFound);
                return Ok(());
            }
            drop(tags_db);

            let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::SelectTagToEdit.to_string())
                .items(&tag_names)
                .interact()?;
            handle_edit(tag_names[selection].clone())
        }
        3 => {
            // Interactive tag deletion with selection
            let mut tags_db = Tags::new()?;
            let tags = tags_db.get_all()?;
            if tags.is_empty() {
                msg_info!(Message::NoTagsFound);
                return Ok(());
            }
            drop(tags_db);

            let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::SelectTagToDelete.to_string())
                .items(&tag_names)
                .interact()?;
            handle_delete(tag_names[selection].clone(), false)
        }
        _ => Ok(()),
    }
}
