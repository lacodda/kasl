use crate::{
    db::tags::{Tag, Tags},
    libs::{messages::Message, view::View},
    msg_error, msg_info, msg_print, msg_success,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

#[derive(Debug, Args)]
pub struct TagArgs {
    #[command(subcommand)]
    command: Option<TagCommand>,
}

#[derive(Debug, Subcommand)]
enum TagCommand {
    /// Create a new tag
    Create {
        /// Tag name
        name: String,
        /// Tag color
        #[arg(short, long)]
        color: Option<String>,
    },
    /// List all tags
    List,
    /// Edit a tag
    Edit {
        /// Tag name or ID to edit
        tag: String,
    },
    /// Delete a tag
    Delete {
        /// Tag name or ID to delete
        tag: String,
    },
    /// Show tasks with a specific tag
    Tasks {
        /// Tag name
        tag: String,
    },
}

pub async fn cmd(args: TagArgs) -> Result<()> {
    match args.command {
        Some(TagCommand::Create { name, color }) => handle_create(name, color),
        Some(TagCommand::List) => handle_list(),
        Some(TagCommand::Edit { tag }) => handle_edit(tag),
        Some(TagCommand::Delete { tag }) => handle_delete(tag),
        Some(TagCommand::Tasks { tag }) => handle_show_tasks(tag).await,
        None => handle_interactive(),
    }
}

fn handle_create(name: String, color: Option<String>) -> Result<()> {
    let mut tags_db = Tags::new()?;

    // Check if tag already exists
    if tags_db.get_by_name(&name)?.is_some() {
        msg_error!(Message::TagAlreadyExists(name));
        return Ok(());
    }

    let tag = Tag::new(name.clone(), color);
    tags_db.create(&tag)?;

    msg_success!(Message::TagCreated(name));
    Ok(())
}

fn handle_list() -> Result<()> {
    let mut tags_db = Tags::new()?;
    let tags = tags_db.list()?;

    if tags.is_empty() {
        msg_info!(Message::NoTagsFound);
        return Ok(());
    }

    msg_print!(Message::TagListHeader, true);
    View::tags(&tags)?;
    Ok(())
}

fn handle_edit(tag_identifier: String) -> Result<()> {
    let mut tags_db = Tags::new()?;

    // Try to find tag by name or ID
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

    let new_name = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTagName.to_string())
        .default(tag.name.clone())
        .interact_text()?;

    let new_color = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptTagColor.to_string())
        .default(tag.color.unwrap_or_default())
        .allow_empty(true)
        .interact_text()?;

    let color = if new_color.is_empty() { None } else { Some(new_color.as_str()) };

    tags_db.update(tag.id.unwrap(), &new_name, color)?;
    msg_success!(Message::TagUpdated(new_name));
    Ok(())
}

fn handle_delete(tag_identifier: String) -> Result<()> {
    let mut tags_db = Tags::new()?;

    // Try to find tag by name or ID
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

    // Check if tag is used by any tasks
    let task_count = tags_db.get_tasks_with_tag(tag.id.unwrap())?.len();

    let prompt = if task_count > 0 {
        Message::ConfirmDeleteTagWithTasks(tag.name.clone(), task_count)
    } else {
        Message::ConfirmDeleteTag(tag.name.clone())
    };

    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt.to_string())
        .default(false)
        .interact()?;

    if confirmed {
        tags_db.delete(tag.id.unwrap())?;
        msg_success!(Message::TagDeleted(tag.name));
    } else {
        msg_info!(Message::OperationCancelled);
    }

    Ok(())
}

async fn handle_show_tasks(tag_name: String) -> Result<()> {
    let mut tags_db = Tags::new()?;

    let tag = match tags_db.get_by_name(&tag_name)? {
        Some(t) => t,
        None => {
            msg_error!(Message::TagNotFound(tag_name));
            return Ok(());
        }
    };

    let task_ids = tags_db.get_tasks_with_tag(tag.id.unwrap())?;

    if task_ids.is_empty() {
        msg_info!(Message::NoTasksWithTag(tag_name));
        return Ok(());
    }

    use crate::db::tasks::Tasks;
    let tasks = Tasks::new()?.fetch(crate::libs::task::TaskFilter::ByIds(task_ids))?;

    msg_print!(Message::TasksWithTag(tag_name), true);
    View::tasks(&tasks)?;

    Ok(())
}

fn handle_interactive() -> Result<()> {
    let options = vec!["Create tag", "List tags", "Edit tag", "Delete tag"];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::SelectTagAction.to_string())
        .items(&options)
        .interact()?;

    match selection {
        0 => {
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
            let mut tags_db = Tags::new()?;
            let tags = tags_db.list()?;
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
            let mut tags_db = Tags::new()?;
            let tags = tags_db.list()?;
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
            handle_delete(tag_names[selection].clone())
        }
        _ => Ok(()),
    }
}
