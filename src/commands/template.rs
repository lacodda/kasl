//! Task template management command.
//!
//! This module provides comprehensive template management functionality for kasl,
//! enabling users to create, edit, delete, and search reusable task templates.
//! Templates streamline the creation of frequently used tasks by providing
//! predefined values for name, comment, and completion status.
//!
//! ## Template Features
//!
//! - **Template CRUD**: Create, read, update, and delete operations
//! - **Search Functionality**: Find templates by name or content
//! - **Interactive Management**: User-friendly interfaces for all operations
//! - **Validation**: Ensures template data integrity and uniqueness
//! - **Integration**: Seamless integration with task creation workflows

use crate::{
    db::templates::{TaskTemplate, Templates},
    libs::{messages::Message, view::View},
    msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

/// Command-line arguments for template management operations.
///
/// The template command uses subcommands to organize different template
/// management operations, providing a clean and intuitive interface for
/// users to manage their template library.
#[derive(Debug, Args)]
pub struct TemplateArgs {
    #[command(subcommand)]
    command: Option<TemplateCommand>,
}

/// Available template management operations.
///
/// Each subcommand provides specific functionality for template lifecycle
/// management, from creation through deletion, with support for both
/// direct command-line usage and interactive operation.
#[derive(Debug, Subcommand)]
enum TemplateCommand {
    /// Create a new task template
    ///
    /// Creates a new reusable template with specified or interactive values.
    /// Templates provide default values for task creation, streamlining
    /// workflows for frequently created task types.
    Create {
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

    /// Delete a template
    ///
    /// Permanently removes a template from the system. Includes confirmation
    /// prompt to prevent accidental deletion. Template deletion does not
    /// affect tasks that were previously created from the template.
    Delete {
        /// Name of the template to delete
        ///
        /// If not provided, an interactive selection interface will be
        /// presented with all available templates.
        name: Option<String>,
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
/// This function serves as the main dispatcher for template operations,
/// routing to appropriate handlers based on user input. When no subcommand
/// is provided, it enters interactive mode for operation selection.
///
/// ## Operation Routing
///
/// - **Create**: Template creation with validation and uniqueness checking
/// - **List**: Formatted display of all available templates
/// - **Edit**: Interactive or direct template modification
/// - **Delete**: Safe template removal with confirmation
/// - **Search**: Text-based template discovery
/// - **Interactive**: Menu-driven operation selection when no subcommand given
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
/// # Create a new template interactively
/// kasl template create
///
/// # Create template with specific name
/// kasl template create --name daily-standup
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
        Some(TemplateCommand::Create { name }) => handle_create(name),
        Some(TemplateCommand::List) => handle_list(),
        Some(TemplateCommand::Edit { name }) => handle_edit(name),
        Some(TemplateCommand::Delete { name }) => handle_delete(name),
        Some(TemplateCommand::Search { query }) => handle_search(query),
        None => handle_interactive(),
    }
}

/// Handles template creation with validation and uniqueness checking.
///
/// This function manages the complete template creation workflow:
/// 1. **Name Collection**: Gets template name from args or interactive prompt
/// 2. **Uniqueness Validation**: Ensures template name doesn't already exist
/// 3. **Property Collection**: Gathers task name, comment, and completion values
/// 4. **Validation**: Ensures all required fields are properly formatted
/// 5. **Database Storage**: Saves the new template with proper error handling
///
/// ## Template Properties
///
/// Templates store these key properties:
/// - **Name**: Unique identifier for referencing the template
/// - **Task Name**: Default name for tasks created from this template
/// - **Comment**: Default comment/description for tasks
/// - **Completeness**: Default completion percentage (0-100)
///
/// ## Validation Rules
///
/// - Template names must be unique across the entire template library
/// - Task names are required and cannot be empty
/// - Completion values must be between 0 and 100 inclusive
/// - Comments are optional and can be empty
///
/// # Arguments
///
/// * `name` - Optional template name from command line, or None for interactive prompt
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
            if *input >= 0 && *input <= 100 {
                Ok(())
            } else {
                Err(&completeness_range_msg)
            }
        })
        .interact_text()?;

    // Create and save the template
    let template = TaskTemplate::new(name.clone(), task_name, comment, completeness);
    templates_db.create(&template)?;

    msg_success!(Message::TemplateCreated(name));
    Ok(())
}

/// Displays all available templates in a formatted table.
///
/// This function retrieves all templates from the database and presents
/// them in a user-friendly table format showing all relevant properties.
/// The display helps users understand their available templates and
/// their configurations.
///
/// ## Display Format
///
/// The table includes these columns:
/// - **Template Name**: Unique identifier for the template
/// - **Task Name**: Default task name that will be used
/// - **Comment**: Default comment or description
/// - **Completeness**: Default completion percentage
///
/// ## Empty State Handling
///
/// When no templates exist, the function provides helpful guidance
/// about creating the first template rather than displaying an empty table.
fn handle_list() -> Result<()> {
    let mut templates_db = Templates::new()?;
    let templates = templates_db.list()?;

    if templates.is_empty() {
        msg_info!(Message::NoTemplatesFound);
        return Ok(());
    }

    msg_print!(Message::TemplateListHeader, true);
    View::templates(&templates)?;
    Ok(())
}

/// Handles template editing with interactive or direct name specification.
///
/// This function provides comprehensive template editing capabilities:
/// 1. **Template Selection**: Uses provided name or interactive selection
/// 2. **Current State Display**: Shows existing template values
/// 3. **Interactive Editing**: Prompts for new values with current values as defaults
/// 4. **Validation**: Ensures edited values meet requirements
/// 5. **Database Update**: Saves changes with proper error handling
///
/// ## Selection Methods
///
/// - **Direct**: When template name is provided via command line
/// - **Interactive**: When no name is provided, presents selection interface
///
/// ## Editing Interface
///
/// For each editable property, the interface:
/// - Shows the current value as the default
/// - Allows the user to accept current value or enter new one
/// - Validates new values according to template rules
/// - Provides clear feedback about validation errors
///
/// # Arguments
///
/// * `name` - Optional template name to edit, or None for interactive selection
fn handle_edit(name: Option<String>) -> Result<()> {
    let mut templates_db = Templates::new()?;

    // Get template name (direct or interactive selection)
    let name = match name {
        Some(n) => n,
        None => {
            let templates = templates_db.list()?;
            if templates.is_empty() {
                msg_info!(Message::NoTemplatesFound);
                return Ok(());
            }

            let template_names: Vec<String> = templates.iter().map(|t| t.name.clone()).collect();
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::SelectTemplateToEdit.to_string())
                .items(&template_names)
                .interact()?;

            template_names[selection].clone()
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
            if *input >= 0 && *input <= 100 {
                Ok(())
            } else {
                Err(&completeness_range_msg)
            }
        })
        .interact_text()?;

    // Update the template
    let updated_template = TaskTemplate::new(name.clone(), task_name, comment, completeness);
    templates_db.update(&updated_template)?;

    msg_success!(Message::TemplateUpdated(name));
    Ok(())
}

/// Handles safe template deletion with confirmation.
///
/// This function manages the template deletion process with appropriate
/// safety measures to prevent accidental data loss:
/// 1. **Template Selection**: Direct name or interactive selection
/// 2. **Existence Validation**: Ensures template exists before attempting deletion
/// 3. **Confirmation Prompt**: Requires explicit user confirmation
/// 4. **Safe Deletion**: Removes template only after confirmation
/// 5. **User Feedback**: Provides clear feedback about operation result
///
/// ## Safety Features
///
/// - Confirms template exists before showing deletion prompt
/// - Uses clear, unambiguous confirmation language
/// - Defaults to "No" for safety
/// - Provides escape opportunity before actual deletion
/// - Clear feedback about cancellation vs. completion
///
/// # Arguments
///
/// * `name` - Optional template name to delete, or None for interactive selection
fn handle_delete(name: Option<String>) -> Result<()> {
    let mut templates_db = Templates::new()?;

    // Get template name (direct or interactive selection)
    let name = match name {
        Some(n) => n,
        None => {
            let templates = templates_db.list()?;
            if templates.is_empty() {
                msg_info!(Message::NoTemplatesFound);
                return Ok(());
            }

            let template_names: Vec<String> = templates.iter().map(|t| t.name.clone()).collect();
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::SelectTemplateToDelete.to_string())
                .items(&template_names)
                .interact()?;

            template_names[selection].clone()
        }
    };

    // Confirm deletion with user
    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::ConfirmDeleteTemplate(name.clone()).to_string())
        .default(false)
        .interact()?;

    if confirmed {
        templates_db.delete(&name)?;
        msg_success!(Message::TemplateDeleted(name));
    } else {
        msg_info!(Message::OperationCancelled);
    }

    Ok(())
}

/// Handles template search functionality.
///
/// This function performs text-based searching across template names and
/// task names, returning all matches in a formatted display. The search
/// is case-insensitive and supports partial matching for user convenience.
///
/// ## Search Algorithm
///
/// The search functionality:
/// - Performs case-insensitive matching
/// - Searches both template names and task names
/// - Supports partial string matching
/// - Returns all matching templates
/// - Displays results in standard template table format
///
/// ## Search Scope
///
/// The search covers these template fields:
/// - **Template Name**: The unique identifier
/// - **Task Name**: The default task name
///
/// Comments and completion values are not included in search to focus
/// on the most relevant identifying information.
///
/// # Arguments
///
/// * `query` - Search string to match against template and task names
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
///
/// This function provides a menu-driven interface for users who prefer
/// interactive operation over command-line arguments. It presents all
/// available template operations in an easy-to-navigate menu format.
///
/// ## Interactive Menu
///
/// The menu presents these options:
/// 1. **Create new template**: Launches template creation workflow
/// 2. **List templates**: Shows all available templates
/// 3. **Edit template**: Template selection and editing interface
/// 4. **Delete template**: Template selection and safe deletion
///
/// Each menu option delegates to the appropriate specialized handler
/// function, ensuring consistent behavior between interactive and
/// command-line usage.
///
/// ## User Experience
///
/// The interactive mode is designed for:
/// - Users new to the command-line interface
/// - Occasional template management tasks
/// - Exploration of available template operations
/// - Situations where remembering exact command syntax is inconvenient
fn handle_interactive() -> Result<()> {
    let options = vec!["Create new template", "List templates", "Edit template", "Delete template"];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::SelectTemplateAction.to_string())
        .items(&options)
        .interact()?;

    match selection {
        0 => handle_create(None),
        1 => handle_list(),
        2 => handle_edit(None),
        3 => handle_delete(None),
        _ => Ok(()),
    }
}
