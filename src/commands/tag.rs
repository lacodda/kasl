//! Tag management command for task organization and categorization.
//!
//! Provides comprehensive tag management functionality, enabling users to create, organize, and utilize tags for better task categorization.
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
#[derive(Debug, Args)]
pub struct TagArgs {
    #[command(subcommand)]
    command: Option<TagCommand>,
}

/// Available tag management operations.
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
