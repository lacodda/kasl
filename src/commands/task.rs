use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct TaskArgs {
    #[arg(required = true)]
    name: String,
}

pub fn cmd(task_args: TaskArgs) -> Result<(), Box<dyn Error>> {
    println!("Task {}", &task_args.name);

    Ok(())
}
