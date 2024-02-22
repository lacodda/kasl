use crate::libs::scheduler::Scheduler;
use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(short, long)]
    delete: bool,
}

pub fn cmd(init_args: InitArgs) -> Result<(), Box<dyn Error>> {
    println!("Init");
    if init_args.delete {
        Scheduler::delete()?;

        return Ok(());
    }
    Scheduler::new()?;

    Ok(())
}
