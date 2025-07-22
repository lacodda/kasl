pub mod init;
pub mod pauses;
pub mod report;
pub mod sum;
pub mod task;
pub mod update;
pub mod watch;

use crate::{db::workdays::Workdays, libs::messages::types::Message, msg_info};
use anyhow::Result;
use chrono::Local;
use clap::{Parser, Subcommand};

/// Defines the main subcommands that the application can execute.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Initializes the application configuration.
    #[command(about = "Configuration initialization")]
    Init(init::InitArgs),

    /// Creates a new task.
    #[command(about = "Create task")]
    Task(task::TaskArgs),

    /// Write end timestamp to database.
    #[command(about = "Write end timestamp to database")]
    End,

    /// Generates a summary of work activities.
    #[command(about = "Get summary")]
    Sum(sum::SumArgs),

    /// Updates the application to the latest version from GitHub releases.
    #[command(about = "Update the application to the latest version")]
    Update,

    /// Prepares and optionally sends a work report.
    #[command(about = "Prepare a report")]
    Report(report::ReportArgs),

    /// Watches for user activity to automatically record pauses.
    #[command(about = "Watch user activity in the background to record pauses")]
    Watch(watch::WatchArgs),

    /// Displays recorded pauses for a given date.
    #[command(about = "Display pauses for a given date")]
    Pauses(pauses::PausesArgs),
}

/// The main CLI structure that parses command-line arguments.
///
/// It uses `clap` to define the application's interface and delegates
/// command execution to the appropriate subcommand module.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help(true))]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    /// Parses the command-line arguments and executes the corresponding command.
    ///
    /// This is the main entry point for the CLI logic.
    pub async fn menu() -> Result<()> {
        let cli = Self::parse();
        match cli.command {
            Commands::Init(args) => init::cmd(args),
            Commands::Task(args) => task::cmd(args).await,
            Commands::End => {
                Workdays::new()?.insert_end(Local::now().date_naive())?;
                msg_info!(Message::WorkdayEnded);
                Ok(())
            }
            Commands::Sum(args) => sum::cmd(args).await,
            Commands::Report(args) => report::cmd(args).await,
            Commands::Update => update::cmd().await,
            Commands::Watch(args) => watch::cmd(args).await,
            Commands::Pauses(args) => pauses::cmd(args).await,
        }
    }
}
