//! Main entry point for the kasl application.
//!
//! This module handles:
//! - Initialization of the tracing system for logging
//! - Command-line argument parsing
//! - Daemon mode execution for background monitoring
//! - Update notifications for new versions

use anyhow::Result;
use kasl::commands::Cli;
use kasl::libs::update::Updater;
use std::env;

/// Main function that initializes the application.
///
/// The function performs the following tasks:
/// 1. Sets up conditional logging based on environment variables
/// 2. Checks if the application is running in daemon mode
/// 3. Shows update notifications if a new version is available
/// 4. Delegates to the CLI handler for command execution
///
/// # Environment Variables
///
/// - `KASL_DEBUG`: Enables debug logging when set
/// - `RUST_LOG`: Standard Rust logging configuration
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing only if debug mode is enabled
    if env::var("KASL_DEBUG").is_ok() || env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "kasl=debug".into()))
            .init();
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--daemon-run" {
        kasl::commands::watch::run_as_daemon().await?;
    } else {
        Updater::show_update_notification().await;
        Cli::menu().await?;
    }
    Ok(())
}
