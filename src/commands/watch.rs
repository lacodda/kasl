//! Contains the logic for the `watch` command.
//!
//! This module handles starting, stopping, and running the activity monitor
//! both as a foreground process for debugging and as a background daemon
//! for normal use.

use crate::libs::{config::Config, daemon, messages::Message, monitor::Monitor};
use crate::msg_print;
use anyhow::Result;
use clap::Args;

/// Command-line arguments for the `watch` command.
#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Runs the watcher in the foreground for debugging instead of as a background process.
    #[arg(long)]
    foreground: bool,

    /// Stops any running background watcher process.
    #[arg(long, short)]
    stop: bool,
}

/// Main entry point for the `watch` command, acting as a dispatcher.
pub async fn cmd(args: WatchArgs) -> Result<()> {
    if args.stop {
        daemon::stop()?;
    } else if args.foreground {
        msg_print!(Message::WatcherStartingForeground);
        run_monitor().await?;
    } else {
        daemon::spawn()?;
    }
    Ok(())
}

/// The core logic that initializes and runs the activity monitor.
/// This function is called either directly for foreground mode or by the daemon process.
async fn run_monitor() -> Result<()> {
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();
    let mut monitor = Monitor::new(monitor_config)?;
    monitor.run().await
}

/// This function is executed by the detached daemon process.
/// It's called from `main.rs` when the `--daemon-run` argument is detected.
pub async fn run_as_daemon() -> Result<()> {
    daemon::run_with_signal_handling().await
}
