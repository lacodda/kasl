use crate::libs::db::Db;
use clap::Args;
use std::error::Error;

#[derive(Debug)]
pub struct Task {
    pub id: Option<i32>,
    pub timestamp: Option<String>,
    pub name: String,
    pub comment: String,
    pub completeness: i32,
}

impl Task {
    fn new(name: &str, comment: &str, completeness: &i32) -> Self {
        Task {
            id: None,
            timestamp: None,
            name: name.to_string(),
            comment: comment.to_string(),
            completeness: completeness.to_owned(),
        }
    }
}

#[derive(Debug, Args)]
pub struct TaskArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(task_args: TaskArgs) -> Result<(), Box<dyn Error>> {
    let task = Task::new(&task_args.name, "", &10);
    let _ = Db::new()?.insert_task(&task);

    println!("Task {}", &task_args.name);

    Ok(())
}
