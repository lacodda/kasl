use crate::{
    db::tasks::Tasks,
    libs::task::{Task, TaskFilter},
};
use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct TaskArgs {
    name: String,
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
    let task = Task::new(&task_args.name, "", None);
    let _ = Tasks::new()?.insert(&task);

    println!("Task name: {}", &task_args.name);

    Ok(())
}
