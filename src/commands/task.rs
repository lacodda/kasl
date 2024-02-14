use crate::{
    db::tasks::Tasks,
    libs::task::{Task, TaskFilter},
};
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Input};
use std::error::Error;

#[derive(Debug, Args)]
pub struct TaskArgs {
    #[arg(short, long)]
    name: Option<String>,
    #[arg(short, long)]
    comment: Option<String>,
    #[arg(short, long)]
    completeness: Option<i32>,
    #[arg(short, long)]
    show: bool,
    #[arg(short, long)]
    all: bool,
    #[arg(short, long)]
    id: Option<Vec<i32>>,
}

pub fn cmd(task_args: TaskArgs) -> Result<(), Box<dyn Error>> {
    if task_args.show {
        let mut filter: TaskFilter = TaskFilter::Today;
        if task_args.all {
            filter = TaskFilter::All;
        } else if task_args.id.is_some() {
            filter = TaskFilter::ByIds(task_args.id.unwrap());
        }
        let tasks = Tasks::new()?.fetch(filter)?;
        println!("Tasks:\n {:?}", &tasks);

        return Ok(());
    }

    let name = task_args
        .name
        .unwrap_or_else(|| Input::with_theme(&ColorfulTheme::default()).with_prompt("Enter task name").interact_text().unwrap());
    let comment = task_args.comment.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .allow_empty(true)
            .with_prompt("Enter comment")
            .interact_text()
            .unwrap()
    });
    let completeness = task_args.completeness.unwrap_or_else(|| {
        Input::with_theme(&ColorfulTheme::default())
            .allow_empty(true)
            .with_prompt("Enter completeness")
            .default(100)
            .interact_text()
            .unwrap()
    });

    let task = Task::new(&name, &comment, Some(completeness));
    let _ = Tasks::new()?.insert(&task);

    println!("Task name: {}", &name);

    Ok(())
}
