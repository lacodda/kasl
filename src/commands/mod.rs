//! Command-line interface commands for kasl application.
//!
//! Contains all CLI command implementations for task management, activity monitoring,
//! reporting, and system configuration.
//!
//! ## Usage
//!
//! ```bash
//! kasl watch                    # Start activity monitoring
//! kasl task --name "Review code" # Create a new task
//! kasl report                   # Generate today's report
//! kasl export tasks --format csv # Export tasks to CSV
//! ```

pub mod autostart;
pub mod export;
pub mod inbox;
pub mod init;
pub mod migrations;
pub mod pauses;
pub mod report;
pub mod sum;
pub mod tag;
pub mod task;
pub mod template;
pub mod update;
pub mod watch;

use crate::{db::workdays::Workdays, libs::messages::types::Message, msg_info, msg_warning};
use anyhow::Result;
use chrono::Local;
use clap::{Parser, Subcommand};

/// Defines the main subcommands that the application can execute.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Manage autostart configuration for system boot
    ///
    /// Controls whether kasl automatically starts monitoring when the system boots.
    /// Supports both system-level and user-level autostart on Windows.
    #[command(about = "Manage autostart on system boot")]
    Autostart(autostart::AutostartArgs),

    /// Set up application configuration interactively
    ///
    /// Guides the user through setting up API credentials, monitor settings,
    /// and other configuration options required for kasl to function properly.
    ///
    /// `init` stays as a deprecated alias until 2.0. An alias rather than a
    /// hidden variant: clap_complete emits hidden subcommands into the
    /// completion scripts, so Tab would keep teaching the old spelling.
    #[command(about = "Set up configuration", alias = "init")]
    Setup(init::SetupArgs),

    /// Comprehensive task management command
    ///
    /// Handles all task-related operations including creation, editing, deletion,
    /// viewing, and integration with external services like GitLab and Jira.
    #[command(about = "Create task")]
    Task(task::TaskArgs),

    /// Manually end the current workday
    ///
    /// Records the end timestamp for today's work session. Typically used
    /// when the automatic monitoring needs to be manually finalized.
    #[command(about = "Write end timestamp to database")]
    End,

    /// Display monthly working hours summary
    ///
    /// Shows a comprehensive overview of work hours, productivity metrics,
    /// and daily breakdowns for the current month.
    #[command(about = "Get summary")]
    Sum(sum::SumArgs),

    /// Update kasl itself to the latest release
    ///
    /// Checks GitHub releases for newer versions and automatically downloads
    /// and installs updates if available.
    ///
    /// `update` stays as a deprecated alias until 2.0; it read as "update my
    /// data", which is what every other command does.
    #[command(name = "self-update", about = "Update kasl itself to the latest release", alias = "update")]
    SelfUpdate,

    /// Generate and optionally submit work reports
    ///
    /// Creates detailed daily reports with work intervals, tasks, and productivity
    /// metrics. Can automatically submit reports to configured APIs.
    #[command(about = "Prepare a report")]
    Report(report::ReportArgs),

    /// Export application data to external formats
    ///
    /// Supports exporting tasks, reports, and summaries to CSV, JSON, and Excel
    /// formats for external analysis or backup purposes.
    #[command(about = "Export data to various formats")]
    Export(export::ExportArgs),

    /// Manage reusable task templates
    ///
    /// Create, edit, and use templates for frequently created tasks to
    /// streamline task creation workflow.
    #[command(about = "Manage task templates")]
    Template(template::TemplateArgs),

    /// Organize tasks with custom tags
    ///
    /// Create and manage tags to categorize and filter tasks by project,
    /// priority, or any custom criteria.
    #[command(about = "Manage task tags")]
    Tag(tag::TagArgs),

    /// Background activity monitoring daemon
    ///
    /// Monitors user input activity to automatically detect work sessions,
    /// breaks, and workday boundaries. Can run as a background service.
    #[command(about = "Watch user activity in the background to record pauses")]
    Watch(watch::WatchArgs),

    /// View recorded pauses and record ones the monitor missed
    ///
    /// Lists detected pauses for a date, and lets the user add an absence the
    /// activity monitor did not catch or remove one recorded by mistake.
    #[command(about = "View pauses and record ones the monitor missed")]
    Pauses(pauses::PausesArgs),

    /// Jira inbox of assigned open issues
    ///
    /// Syncs assigned unresolved Jira issues into a local table, lists them
    /// by priority, and supports pin / dismiss / open / import into tasks.
    #[command(about = "Manage Jira inbox issues")]
    Inbox(inbox::InboxArgs),

    /// Print a shell completion script
    ///
    /// Emits the completion script for the chosen shell on stdout; source it
    /// from the shell profile to get completion for kasl's commands and flags.
    #[command(about = "Print a shell completion script (source it from your shell profile)")]
    Completions {
        /// Target shell
        shell: clap_complete::Shell,
    },

    /// Database migration management utilities (debug builds only)
    ///
    /// Provides tools for database schema management, migration history,
    /// and rollback operations. Available only in debug builds for safety.
    #[cfg(debug_assertions)]
    #[command(about = "Database migration management")]
    Migrations(migrations::MigrationsArgs),
}

/// The main CLI structure that parses command-line arguments.
///
/// Uses `clap` to define the application's interface and delegates
/// command execution to the appropriate subcommand module. The CLI
/// requires at least one subcommand to be specified.
///
/// # Examples
///
/// ```bash
/// # Display help
/// kasl --help
///
/// # Run a specific command
/// kasl task --name "New task"
/// ```
#[derive(Debug, Parser)]
#[command(name = "kasl", author, version, about, long_about = None)]
#[command(arg_required_else_help(true))]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    /// Parses command-line arguments and executes the corresponding command.
    ///
    /// This is the main entry point for the CLI logic. It handles command
    /// routing and provides centralized error handling for all commands.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::commands::Cli;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     Cli::menu().await
    /// }
    /// ```
    pub async fn menu() -> Result<()> {
        let cli = Self::parse();

        match cli.command {
            Commands::Autostart(args) => autostart::cmd(args),
            Commands::Setup(args) => {
                warn_if_deprecated_alias();
                init::cmd(args)
            }
            Commands::Task(args) => task::cmd(args).await,
            Commands::End => {
                // Manually end the current workday
                Workdays::new()?.insert_end(Local::now().date_naive())?;
                msg_info!(Message::WorkdayEnded);
                Ok(())
            }
            Commands::Sum(args) => sum::cmd(args).await,
            Commands::Report(args) => report::cmd(args).await,
            Commands::Export(args) => export::cmd(args).await,
            Commands::Template(args) => template::cmd(args),
            Commands::Tag(args) => tag::cmd(args).await,
            Commands::SelfUpdate => {
                warn_if_deprecated_alias();
                update::cmd().await
            }
            Commands::Watch(args) => watch::cmd(args).await,
            Commands::Pauses(args) => pauses::cmd(args).await,
            Commands::Completions { shell } => {
                use clap::CommandFactory;
                clap_complete::generate(shell, &mut Self::command(), "kasl", &mut std::io::stdout());
                Ok(())
            }
            Commands::Inbox(args) => inbox::cmd(args).await,

            // Database migrations only available in debug builds
            #[cfg(debug_assertions)]
            Commands::Migrations(args) => migrations::cmd(args),
        }
    }
}

/// Old command names still accepted as aliases, with their replacements.
///
/// Removed in 2.0; until then the alias works and says so.
const DEPRECATED_ALIASES: [(&str, &str); 2] = [("init", "setup"), ("update", "self-update")];

/// Prints a rename notice when the command was invoked by its old name.
///
/// clap resolves an alias to the canonical variant without recording which
/// spelling was typed, so the first non-flag argument is what tells them
/// apart.
fn warn_if_deprecated_alias() {
    let Some(typed) = std::env::args().nth(1) else {
        return;
    };
    if let Some((old, new)) = DEPRECATED_ALIASES.iter().find(|(old, _)| *old == typed) {
        msg_warning!(Message::DeprecatedCommand(old, new));
    }
}
