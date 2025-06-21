use crate::libs::config::Config;
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
        return Ok(());
    }
    Config::init()?.save()?;
    Ok(())
}
