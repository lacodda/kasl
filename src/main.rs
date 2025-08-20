//! Main entry point for the kasl application.
//!
//! Handles initialization of the tracing system, command-line argument parsing,
//! daemon mode execution, and update notifications.

use anyhow::Result;
use kasl::commands::Cli;
use kasl::libs::update::Updater;
use std::env;

/// Main function that initializes the application.
///
/// Sets up logging, checks for daemon mode, shows update notifications,
/// and delegates to CLI handler for command execution.
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing only if debug mode is enabled
    // This prevents log output from cluttering normal CLI usage
    if env::var("KASL_DEBUG").is_ok() || env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "kasl=debug".into()))
            .init();
    }

    // Check for daemon mode flag - this is used internally when spawning
    // the background monitoring process
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--daemon-run" {
        // Run as daemon process for background activity monitoring
        kasl::commands::watch::run_as_daemon().await?;
    } else {
        // Normal CLI execution

        // Check for application updates in the background
        // This is non-blocking and will only show notifications
        Updater::show_update_notification().await;

        // Parse and execute CLI commands
        Cli::menu().await?;
    }

    Ok(())
}
