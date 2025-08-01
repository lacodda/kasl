//! Activity monitoring and daemon management command.
//!
//! This module handles the core functionality of kasl - monitoring user activity
//! to automatically detect work sessions, breaks, and workday boundaries. It supports
//! both foreground debugging mode and background daemon operation.
//!
//! ## Monitoring Process
//!
//! The watch command operates by:
//! 1. **Input Detection**: Monitors keyboard and mouse activity using system APIs
//! 2. **Activity Analysis**: Determines work vs. break periods based on configurable thresholds
//! 3. **Database Recording**: Automatically records workday start/end times and pause periods
//! 4. **State Management**: Maintains activity state across application restarts
//!
//! ## Operating Modes
//!
//! - **Background Daemon**: Runs silently in the background, perfect for daily use
//! - **Foreground Debug**: Runs in the terminal with detailed logging for troubleshooting
//! - **Stop Mode**: Terminates any running background processes

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
/// This function acts as a dispatcher that routes to the appropriate operation
/// based on the provided command-line arguments. It handles the three main
/// operational modes of the watch command.
///
/// # Arguments
///
/// * `args` - Parsed command-line arguments specifying the operation mode
///
/// # Returns
///
/// Returns `Ok(())` on successful operation completion, or an error if
/// the requested operation fails.
///
/// # Examples
///
/// ```bash
/// # Start background monitoring
/// kasl watch
///
/// # Run in foreground with debug output
/// kasl watch --foreground
///
/// # Stop background monitoring
/// kasl watch --stop
/// ```
///
/// # Operation Modes
///
/// 1. **Stop Mode** (`--stop`): Terminates background daemon
/// 2. **Foreground Mode** (`--foreground`): Runs interactively with logging
/// 3. **Default Mode**: Spawns background daemon process
///
/// # Error Handling
///
/// Common error scenarios:
/// - Daemon already running (for default mode)
/// - No daemon to stop (for stop mode)
/// - Configuration errors (invalid settings)
/// - Permission issues (system API access)
/// - Database connection failures
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
///
/// This function is called either directly for foreground mode or by the daemon
/// process for background operation. It performs the following steps:
///
/// 1. **Configuration Loading**: Reads monitor settings from config file
/// 2. **Monitor Initialization**: Sets up input device listeners and database connections
/// 3. **Main Loop Execution**: Runs the continuous activity monitoring loop
///
/// ## Monitor Configuration
///
/// The monitor behavior is controlled by configuration settings:
/// - `pause_threshold`: Seconds of inactivity before recording a pause
/// - `poll_interval`: Milliseconds between activity checks
/// - `activity_threshold`: Seconds of activity needed to start a workday
/// - `min_pause_duration`: Minimum pause length to record (filters noise)
///
/// ## Activity Detection
///
/// The monitor tracks these input events:
/// - Keyboard presses and releases
/// - Mouse button clicks
/// - Mouse movement
/// - Mouse wheel scrolling
///
/// ## Database Operations
///
/// During monitoring, the system automatically:
/// - Creates workday records when sustained activity is detected
/// - Records pause start times when inactivity threshold is exceeded
/// - Records pause end times when activity resumes
/// - Updates workday end times when monitoring stops
///
/// # Returns
///
/// Returns `Ok(())` when monitoring completes normally, or an error if
/// initialization fails or a critical error occurs during monitoring.
///
/// # Error Scenarios
///
/// - Database connection failures
/// - Input device access denied
/// - Invalid configuration values
/// - System resource exhaustion
#[instrument]
async fn run_monitor() -> Result<()> {
    // Load configuration with defaults for missing values
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();

    // Initialize the activity monitor with configuration
    let mut monitor = Monitor::new(monitor_config)?;

    // Start the main monitoring loop
    // This will run indefinitely until stopped or an error occurs
    monitor.run().await
}

/// Entry point for daemon mode execution.
///
/// This function is called when the application is started with the `--daemon-run`
/// flag, which happens when the main process spawns a background daemon. It sets
/// up proper signal handling for graceful shutdown and runs the monitoring loop.
///
/// ## Signal Handling
///
/// The daemon process responds to these signals:
/// - **SIGTERM**: Graceful shutdown (Unix)
/// - **SIGINT**: Interrupt signal (Unix)
/// - **Ctrl+C**: Console interrupt (Windows)
///
/// ## Process Management
///
/// The daemon:
/// - Detaches from the parent process
/// - Creates a PID file for process tracking
/// - Handles cleanup on shutdown
/// - Logs operations for debugging
///
/// # Returns
///
/// Returns `Ok(())` when the daemon shuts down normally, or an error if
/// startup fails or a critical error occurs.
///
/// # Usage
///
/// This function is called internally by the application and should not be
/// called directly. It's triggered by the `--daemon-run` argument which is
/// used when spawning the background process.
#[instrument]
pub async fn run_as_daemon() -> Result<()> {
    daemon::run_with_signal_handling().await
}
