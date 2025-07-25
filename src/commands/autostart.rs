use crate::libs::{autostart, messages::Message};
use crate::msg_print;
use anyhow::Result;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct AutostartArgs {
    #[command(subcommand)]
    command: AutostartCommand,
}

#[derive(Debug, Subcommand)]
enum AutostartCommand {
    /// Enable autostart on system boot
    Enable,
    /// Disable autostart on system boot
    Disable,
    /// Show current autostart status
    Status,
}

pub fn cmd(args: AutostartArgs) -> Result<()> {
    match args.command {
        AutostartCommand::Enable => {
            autostart::enable()?;
            Ok(())
        }
        AutostartCommand::Disable => {
            autostart::disable()?;
            Ok(())
        }
        AutostartCommand::Status => {
            let status = autostart::status()?;
            msg_print!(Message::AutostartStatus(status));
            Ok(())
        }
    }
}
