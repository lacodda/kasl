use crate::{db::tasks::Tasks, libs::task::Task};
use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct TaskArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(task_args: TaskArgs) -> Result<(), Box<dyn Error>> {
    let task = Task::new(&task_args.name, "", &10);
    let _ = Tasks::new()?.insert(&task);

    println!("Task {}", &task_args.name);

    Ok(())
}
