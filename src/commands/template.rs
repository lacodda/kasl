use crate::{
    db::templates::{TaskTemplate, Templates},
    libs::{messages::Message, view::View},
    msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

#[derive(Debug, Args)]
pub struct TemplateArgs {
    #[command(subcommand)]
    command: Option<TemplateCommand>,
}

#[derive(Debug, Subcommand)]
enum TemplateCommand {
    /// Create a new template
    Create {
        /// Template name (unique identifier)
        #[arg(short, long)]
        name: Option<String>,
    },
    /// List all templates
    List,
    /// Edit an existing template
    Edit {
        /// Template name to edit
        name: Option<String>,
    },
    /// Delete a template
    Delete {
        /// Template name to delete
        name: Option<String>,
    },
    /// Search templates
    Search {
        /// Search query
        query: String,
    },
}

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

fn handle_create(name: Option<String>) -> Result<()> {
    let mut templates_db = Templates::new()?;

    let name = name.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptTemplateName.to_string())
            .interact_text()
            .unwrap()
    });

    // Check if template already exists
    if templates_db.exists(&name)? {
        msg_error!(Message::TemplateAlreadyExists(name));
        return Ok(());
    }

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

    let template = TaskTemplate::new(name.clone(), task_name, comment, completeness);
    templates_db.create(&template)?;

    msg_success!(Message::TemplateCreated(name));
    Ok(())
}

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

fn handle_edit(name: Option<String>) -> Result<()> {
    let mut templates_db = Templates::new()?;

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

    let template = match templates_db.get(&name)? {
        Some(t) => t,
        None => {
            msg_error!(Message::TemplateNotFound(name));
            return Ok(());
        }
    };

    msg_print!(Message::EditingTemplate(template.name.clone()), true);

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

    let updated_template = TaskTemplate::new(name.clone(), task_name, comment, completeness);
    templates_db.update(&updated_template)?;

    msg_success!(Message::TemplateUpdated(name));
    Ok(())
}

fn handle_delete(name: Option<String>) -> Result<()> {
    let mut templates_db = Templates::new()?;

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
