//! Task template management command.
//!
//! Provides comprehensive template management functionality for kasl, enabling users to create, edit, delete, and search reusable task templates.
//!
//! ## Usage
//!
//! ```bash
//! # List all templates
//! kasl template list
//!
//! # Create new template
//! kasl template add --name "bug-fix"
//!
//! # Search templates
//! kasl template search "development"
//!
//! # Delete template
//! kasl template remove "old-template"
//! ```

use crate::{
    db::templates::{TaskTemplate, Templates},
    libs::{messages::Message, pick, prompt::ensure_interactive, view::View},
    msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};

/// Command-line arguments for template management operations.
#[derive(Debug, Args)]
pub struct TemplateArgs {
    #[command(subcommand)]
    command: Option<TemplateCommand>,
}

/// Available template management operations.
#[derive(Debug, Subcommand)]
enum TemplateCommand {
    /// Add a new task template
    ///
    /// Creates a new reusable template with specified or interactive values.
    /// Templates provide default values for task creation, streamlining
    /// workflows for frequently created task types.
    Add {
        /// Unique name identifier for the template
        ///
        /// Must be unique across all templates and should be descriptive
        /// enough to easily identify the template's purpose. Used for
        /// referencing the template in task creation commands.
        #[arg(short, long)]
        name: Option<String>,
    },

    /// List all available templates
    ///
    /// Displays a formatted table of all existing templates with their
    /// names, task names, comments, and default completion values.
    /// Useful for reviewing available templates and their configurations.
    List,

    /// Show a single template's contents
    ///
    /// Displays the task name, comment and default completeness stored in
    /// the template, so its effect on task creation is visible before use.
    Show {
        /// Name of the template to show
        name: Option<String>,
    },

    /// Edit an existing template
    ///
    /// Modifies an existing template's properties including task name,
    /// comment, and completion status. Provides interactive interface
    /// for template selection if name is not specified.
    Edit {
        /// Name of the template to edit
        ///
        /// If not provided, an interactive selection interface will be
        /// presented with all available templates.
        name: Option<String>,
    },

    /// Remove a template
    ///
    /// Permanently removes a template from the system. Includes confirmation
    /// prompt to prevent accidental removal. Removing a template does not
    /// affect tasks that were previously created from it.
    Remove {
        /// Name of the template to remove
        ///
        /// If not provided, an interactive selection interface will be
        /// presented with all available templates.
        name: Option<String>,

        /// Remove without asking for confirmation
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Search templates by name or content
    ///
    /// Performs a text search across template names and task names,
    /// returning all matching templates. Useful for finding templates
    /// in large template libraries.
    Search {
        /// Search query string
        ///
        /// Searches both template names and task names for matches.
        /// Case-insensitive partial matching is supported.
        query: String,
    },
}

/// Executes template management operations based on the specified subcommand.
///
/// # Examples
///
/// ```bash
/// # Create a new template interactively
/// kasl template add
///
/// # Create template with specific name
/// kasl template add --name daily-standup
///
/// # List all templates
/// kasl template list
///
/// # Edit a template
/// kasl template edit daily-standup
///
/// # Search for templates
/// kasl template search meeting
///
/// # Interactive mode
/// kasl template
/// ```
pub fn cmd(args: TemplateArgs) -> Result<()> {
    match args.command {
        Some(TemplateCommand::Add { name }) => handle_create(name),
        Some(TemplateCommand::List) => handle_list(),
        Some(TemplateCommand::Show { name }) => handle_show(name),
        Some(TemplateCommand::Edit { name }) => handle_edit(name),
        Some(TemplateCommand::Remove { name, yes }) => handle_delete(name, yes),
        Some(TemplateCommand::Search { query }) => handle_search(query),
        None => {
            ensure_interactive("no subcommand given; run `kasl template list` or see `kasl template --help`")?;
            handle_interactive()
        }
    }
}

/// Handles template creation with validation and uniqueness checking.
fn handle_create(name: Option<String>) -> Result<()> {
    let mut templates_db = Templates::new()?;

    // Get template name (from args or interactive prompt)
    let name = name.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptTemplateName.to_string())
            .interact_text()
            .unwrap()
    });

    // Validate template name uniqueness
    if templates_db.exists(&name)? {
        msg_error!(Message::TemplateAlreadyExists(name));
        return Ok(());
    }

    // Collect template properties interactively
    let task_name = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTemplateTaskName.to_string())
        .interact_text()?;

    let comment = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTemplateComment.to_string())
        .allow_empty(true)
        .interact_text()?;

    let completeness_range_msg = Message::TaskCompletenessRange.to_string();
    let completeness = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTemplateCompleteness.to_string())
        .default(100)
        .validate_with(|input: &i32| -> Result<(), &str> {
            if *input >= 0 && *input <= 100 { Ok(()) } else { Err(&completeness_range_msg) }
        })
        .interact_text()?;

    // Create and save the template
    let template = TaskTemplate::new(name.clone(), task_name, comment, completeness);
    templates_db.create(&template)?;

    msg_success!(Message::TemplateCreated(name));
    Ok(())
}

/// Displays all available templates in a formatted table.
fn handle_list() -> Result<()> {
    let mut templates_db = Templates::new()?;
    let templates = templates_db.get_all()?;

    if templates.is_empty() {
        msg_info!(Message::NoTemplatesFound);
        return Ok(());
    }

    msg_print!(Message::TemplateListHeader, true);
    View::templates(&templates)?;
    Ok(())
}

/// Displays a single template's stored values.
///
/// Reuses the same table rendering as `list` so a template reads identically
/// whether shown alone or among others. Without a name, the template is picked
/// interactively.
fn handle_show(name: Option<String>) -> Result<()> {
    let mut templates_db = Templates::new()?;

    let name = match name {
        Some(n) => n,
        None => {
            ensure_interactive("template name is required outside an interactive terminal")?;

            let templates = templates_db.get_all()?;
            if templates.is_empty() {
                msg_info!(Message::NoTemplatesFound);
                return Ok(());
            }

            pick::template(&templates, &Message::SelectTemplate.to_string())?
        }
    };

    match templates_db.get(&name)? {
        Some(template) => {
            msg_print!(Message::TemplateListHeader, true);
            View::templates(&[template])?;
        }
        None => msg_error!(Message::TemplateNotFound(name)),
    }

    Ok(())
}

/// Handles template editing with interactive or direct name specification.
fn handle_edit(name: Option<String>) -> Result<()> {
    let mut templates_db = Templates::new()?;

    // Get template name (direct or interactive selection)
    let name = match name {
        Some(n) => n,
        None => {
            let templates = templates_db.get_all()?;
            if templates.is_empty() {
                msg_info!(Message::NoTemplatesFound);
                return Ok(());
            }

            pick::template(&templates, &Message::SelectTemplateToEdit.to_string())?
        }
    };

    // Fetch the template to edit
    let template = match templates_db.get(&name)? {
        Some(t) => t,
        None => {
            msg_error!(Message::TemplateNotFound(name));
            return Ok(());
        }
    };

    msg_print!(Message::EditingTemplate(template.name.clone()), true);

    // Interactive editing with current values as defaults
    let task_name = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTemplateTaskName.to_string())
        .default(template.task_name.clone())
        .interact_text()?;

    let comment = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTemplateComment.to_string())
        .default(template.comment.clone())
        .allow_empty(true)
        .interact_text()?;

    let completeness_range_msg = Message::TaskCompletenessRange.to_string();
    let completeness = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTemplateCompleteness.to_string())
        .default(template.completeness)
        .validate_with(|input: &i32| -> Result<(), &str> {
            if *input >= 0 && *input <= 100 { Ok(()) } else { Err(&completeness_range_msg) }
        })
        .interact_text()?;

    // Update the template
    let updated_template = TaskTemplate::new(name.clone(), task_name, comment, completeness);
    templates_db.update(&updated_template)?;

    msg_success!(Message::TemplateUpdated(name));
    Ok(())
}

/// Handles safe template deletion with confirmation.
fn handle_delete(name: Option<String>, assume_yes: bool) -> Result<()> {
    let mut templates_db = Templates::new()?;

    // Get template name (direct or interactive selection)
    let name = match name {
        Some(n) => n,
        None => {
            ensure_interactive("template name is required outside an interactive terminal")?;

            let templates = templates_db.get_all()?;
            if templates.is_empty() {
                msg_info!(Message::NoTemplatesFound);
                return Ok(());
            }

            pick::template(&templates, &Message::SelectTemplateToDelete.to_string())?
        }
    };

    if !assume_yes {
        // Never block on a prompt when there is no one to answer it.
        ensure_interactive(&format!("refusing to remove template '{}' without --yes outside an interactive terminal", name))?;

        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::ConfirmDeleteTemplate(name.clone()).to_string())
            .default(false)
            .interact()?;

        if !confirmed {
            msg_info!(Message::OperationCancelled);
            return Ok(());
        }
    }

    templates_db.delete(&name)?;
    msg_success!(Message::TemplateDeleted(name));

    Ok(())
}

/// Handles template search functionality.
fn handle_search(query: String) -> Result<()> {
    let mut templates_db = Templates::new()?;
    let templates = templates_db.search(&query)?;

    if templates.is_empty() {
        msg_info!(Message::NoTemplatesMatchingQuery(query));
        return Ok(());
    }

    msg_print!(Message::TemplateSearchResults(query), true);
    View::templates(&templates)?;
    Ok(())
}

/// Handles interactive template management when no subcommand is provided.
fn handle_interactive() -> Result<()> {
    let options = vec!["Add new template", "List templates", "Show template", "Edit template", "Remove template"];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::SelectTemplateAction.to_string())
        .items(&options)
        .interact()?;

    match selection {
        0 => handle_create(None),
        1 => handle_list(),
        2 => handle_show(None),
        3 => handle_edit(None),
        4 => handle_delete(None, false),
        _ => Ok(()),
    }
}
