mod commands;
use clap::{Parser, Subcommand};
use commands::{event, init, login, logout, report, sum, sync, task};
use std::error::Error;
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
    #[command(about = "Configuration initialization", arg_required_else_help = true)]
    Init(init::InitArgs),
    #[command(about = "Authenticate to the finlab server", arg_required_else_help = true)]
    Login(login::LoginArgs),
    #[command(about = "Logs out the user by removing the user's session from local state", arg_required_else_help = true)]
    Logout(logout::LogoutArgs),
    #[command(about = "Create task")]
    Task(task::TaskArgs),
    #[command(about = "Write timestamp and event type to database", arg_required_else_help = true)]
    Event(event::EventArgs),
    #[command(about = "Synchronizing local storage with the server", arg_required_else_help = true)]
    Sync(sync::SyncArgs),
    #[command(about = "Get summary", arg_required_else_help = true)]
    Sum(sum::SumArgs),
    #[command(about = "Prepare a report", arg_required_else_help = true)]
    Report(report::ReportArgs),
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init(args) => init::cmd(args),
        Commands::Login(args) => login::cmd(args),
        Commands::Logout(args) => logout::cmd(args),
        Commands::Task(args) => task::cmd(args),
        Commands::Event(args) => event::cmd(args),
        Commands::Sync(args) => sync::cmd(args),
        Commands::Sum(args) => sum::cmd(args),
        Commands::Report(args) => report::cmd(args),
    }
}
