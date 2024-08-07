pub mod event;
pub mod init;
pub mod report;
pub mod sum;
pub mod task;
pub mod update;
pub mod watch;

use crate::libs::event::EventType;
use clap::{Parser, Subcommand};
use event::EventArgs;
use std::error::Error;

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Configuration initialization")]
    Init(init::InitArgs),
    #[command(about = "Create task")]
    Task(task::TaskArgs),
    #[command(about = "Write timestamp and event type to database", arg_required_else_help = true)]
    Event(event::EventArgs),
    #[command(about = "Write start timestamp to database")]
    Start,
    #[command(about = "Write end timestamp to database")]
    End,
    #[command(about = "Get summary")]
    Sum(sum::SumArgs),
    #[command(about = "Update the application to the latest version")]
    Update,
    #[command(about = "Prepare a report")]
    Report(report::ReportArgs),
    #[command(about = "Watch")]
    Watch,
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
            Commands::Event(args) => event::cmd(args),
            Commands::Start => event::cmd(EventArgs {
                event_type: EventType::Start,
                show: false,
                raw: false,
            }),
            Commands::End => event::cmd(EventArgs {
                event_type: EventType::End,
                show: false,
                raw: false,
            }),
            Commands::Sum(args) => sum::cmd(args).await,
            Commands::Report(args) => report::cmd(args).await,
            Commands::Update => update::cmd().await,
            Commands::Watch => Ok(watch::cmd()),
        }
    }
}
