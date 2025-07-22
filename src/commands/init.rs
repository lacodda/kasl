use crate::{libs::{config::Config, messages::Message}, msg_success};
use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(short, long)]
    delete: bool,
}

pub fn cmd(init_args: InitArgs) -> Result<()> {
    let _ = Config::set_app_global();
    if init_args.delete {
        return Ok(());
    }
    Config::init()?.save()?;
    msg_success!(Message::ConfigSaved);
    Ok(())
}
