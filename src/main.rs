mod commands;
use clap::{Parser, Subcommand};
use commands::{
    event::{self, EventArgs},
    init, report, sum, task, watch,
};
use libs::event::EventType;
use std::error::Error;
mod api;
mod db;
mod libs;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
#[command(arg_required_else_help(true))]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

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
    #[command(about = "Prepare a report")]
    Report(report::ReportArgs),
    #[command(about = "Watch")]
    Watch,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init(args) => init::cmd(args),
        Commands::Task(args) => task::cmd(args),
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
        Commands::Sum(args) => sum::cmd(args),
        Commands::Report(args) => report::cmd(args),
        Commands::Watch => Ok(watch::cmd()),
    }
}
