pub mod pauses;
pub mod init;
pub mod report;
pub mod sum;
pub mod task;
pub mod update;
pub mod watch;

use crate::db::workdays::Workdays;
use chrono::Local;
use clap::{Parser, Subcommand};
use std::error::Error;

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Configuration initialization")]
    Init(init::InitArgs),
    #[command(about = "Create task")]
    Task(task::TaskArgs),
    #[command(about = "Write end timestamp to database")]
    End,
    #[command(about = "Get summary")]
    Sum(sum::SumArgs),
    #[command(about = "Update the application to the latest version")]
    Update,
    #[command(about = "Prepare a report")]
    Report(report::ReportArgs),
    #[command(about = "Watch user activity and record pauses")]
    Watch,
    #[command(about = "Display pauses for a given date")]
    Pauses(pauses::PausesArgs),
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help(true))]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub async fn menu() -> Result<(), Box<dyn Error>> {
        let cli = Self::parse();
        match cli.command {
            Commands::Init(args) => init::cmd(args),
            Commands::Task(args) => task::cmd(args).await,
            Commands::End => {
                Workdays::new()?.insert_end(Local::now().date_naive())?;
                println!("Workday ended for today.");
                Ok(())
            }
            Commands::Sum(args) => sum::cmd(args).await,
            Commands::Report(args) => report::cmd(args).await,
            Commands::Update => update::cmd().await,
            Commands::Watch => watch::cmd().await,
            Commands::Pauses(args) => pauses::cmd(args).await,
        }
    }
}
