use crate::libs::{scheduler::Scheduler, config::Config};
use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(short, long)]
    delete: bool,
}

pub fn cmd(init_args: InitArgs) -> Result<(), Box<dyn Error>> {
    let _ = Config::set_app_global();
    if init_args.delete {
        Scheduler::delete()?;

        return Ok(());
    }
    Scheduler::new()?;

    Ok(())
}
