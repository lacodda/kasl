//! System autostart management command.
//!
//! This command provides cross-platform functionality for managing whether
//! kasl automatically starts monitoring when the system boots. It supports
//! different autostart mechanisms depending on the operating system.

use crate::libs::{autostart, messages::Message};
use crate::msg_print;
use anyhow::Result;
use clap::{Args, Subcommand};

/// Command-line arguments for autostart management.
///
/// The autostart command uses subcommands to provide different operations
/// for managing the autostart configuration.
#[derive(Debug, Args)]
pub struct AutostartArgs {
    #[command(subcommand)]
    command: AutostartCommand,
}

/// Available autostart operations.
///
/// Each variant provides a specific autostart management function:
/// - Enable: Set up automatic startup
/// - Disable: Remove automatic startup
/// - Status: Check current autostart state
#[derive(Debug, Subcommand)]
enum AutostartCommand {
    /// Enable autostart on system boot
    ///
    /// Configures the system to automatically start kasl monitoring
    /// when the user logs in or the system boots. The implementation
    /// varies by platform:
    ///
    /// **Windows**:
    /// - Primary: Windows Task Scheduler (requires admin privileges)
    /// - Fallback: Registry Run key (current user)
    ///
    /// **macOS**: LaunchAgent (planned - not yet implemented)
    /// **Linux**: systemd user service (planned - not yet implemented)
    Enable,

    /// Disable autostart on system boot
    ///
    /// Removes any existing autostart configuration, ensuring kasl
    /// will not automatically start on system boot.
    Disable,

    /// Show current autostart status
    ///
    /// Checks and displays whether autostart is currently enabled
    /// or disabled on the system.
    Status,
}

/// Executes the autostart command based on the specified operation.
///
/// This function delegates to the appropriate autostart library function
/// based on the user's choice. It handles platform-specific implementations
/// transparently.
///
/// # Arguments
///
/// * `args` - Parsed command arguments containing the operation to perform
///
/// # Returns
///
/// Returns `Ok(())` on successful operation, or an error if the autostart
/// operation fails (e.g., insufficient privileges, unsupported platform).
///
/// # Examples
///
/// ```bash
/// # Enable autostart
/// kasl autostart enable
///
/// # Check status
/// kasl autostart status
///
/// # Disable autostart
/// kasl autostart disable
/// ```
///
/// # Platform-Specific Behavior
///
/// **Windows**:
/// - Attempts system-level Task Scheduler first (requires admin)
/// - Falls back to user-level Registry entry if admin access denied
/// - Provides clear feedback about which method was used
///
/// **Unix-like Systems**:
/// - Currently returns "not implemented" error
/// - Future versions will support LaunchAgent (macOS) and systemd (Linux)
///
/// # Errors
///
/// Common error scenarios:
/// - Insufficient privileges (Windows Task Scheduler)
/// - Platform not supported (Unix systems)
/// - System configuration conflicts
pub fn cmd(args: AutostartArgs) -> Result<()> {
    match args.command {
        AutostartCommand::Enable => {
            autostart::enable()?;
            Ok(())
        }
        AutostartCommand::Disable => {
            autostart::disable()?;
            Ok(())
        }
        AutostartCommand::Status => {
            let status = autostart::status()?;
            msg_print!(Message::AutostartStatus(status));
            Ok(())
        }
    }
}
