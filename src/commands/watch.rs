//! Activity monitoring and daemon management command.
//!
//! Handles the core functionality of kasl - monitoring user activity to automatically detect work sessions, breaks, and workday boundaries.
//!
//! ## Usage
//!
//! ```bash
//! # Start background monitoring
//! kasl watch
//!
//! # Run in foreground for debugging
//! kasl watch --foreground
//!
//! # Stop background monitoring
//! kasl watch --stop
//! ```

use crate::libs::{config::Config, daemon, messages::Message, monitor::Monitor};
use crate::msg_print;
use anyhow::Result;
use clap::Args;
use tracing::instrument;

/// Command-line arguments for the watch command.
///
/// The watch command provides different operational modes to suit various use cases,
/// from daily background monitoring to debugging and development.
#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Run the watcher in the foreground for debugging
    ///
    /// When specified, the monitor runs in the current terminal session with
    /// enhanced logging output. This is useful for:
    /// - Debugging activity detection issues
    /// - Testing configuration changes
    /// - Development and troubleshooting
    ///
    /// The foreground mode provides real-time feedback about detected activity,
    /// pause events, and workday state changes.
    #[arg(long)]
    foreground: bool,

    /// Stop any running background watcher process
    ///
    /// Terminates the background daemon if it's currently running. This is
    /// useful for:
    /// - Stopping monitoring before system shutdown
    /// - Restarting with new configuration
    /// - Troubleshooting daemon issues
    ///
    /// The stop operation is safe and will properly close database connections
    /// and clean up system resources.
    #[arg(long, short)]
    stop: bool,
}

/// Main entry point for the watch command.
///
/// Acts as a dispatcher that routes to the appropriate operation based on the
/// provided command-line arguments, handling the three main operational modes.
#[instrument]
pub async fn cmd(args: WatchArgs) -> Result<()> {
    if args.stop {
        // Stop any running background daemon
        daemon::stop()?;
    } else if args.foreground {
        // Run in foreground mode with enhanced logging
        msg_print!(Message::WatcherStartingForeground);
        run_monitor().await?;
    } else {
        // Default mode: spawn background daemon
        daemon::spawn()?;
    }
    Ok(())
}

/// Core monitoring logic that initializes and runs the activity monitor.
#[instrument]
async fn run_monitor() -> Result<()> {
    // Load configuration with defaults for missing values
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();

    // Initialize the activity monitor with configuration
    let mut monitor = Monitor::new(monitor_config)?;

    // Sibling poller so foreground mode also keeps the Jira inbox warm
    let inbox_handle = tokio::spawn(async move {
        crate::libs::jira_inbox::run_poller().await;
    });

    let result = monitor.run().await;
    inbox_handle.abort();
    result
}

/// Entry point for daemon mode execution.
///
/// # Usage
#[instrument]
pub async fn run_as_daemon() -> Result<()> {
    daemon::run_with_signal_handling().await
}
