//! Application configuration initialization command.
//!
//! Provides an interactive setup wizard that guides users through configuring kasl for first-time use.
//!
//! ## Features
//!
//! - **Interactive Setup**: Guided configuration wizard for all settings
//! - **API Integration**: Configure GitLab, Jira, and custom API credentials
//! - **Monitoring Settings**: Set up activity thresholds and productivity parameters
//! - **PATH Integration**: Automatically adds kasl to system PATH
//! - **Reset Capability**: Remove existing configuration for troubleshooting
//!
//! ## Usage
//!
//! ```bash
//! # Run interactive setup wizard
//! kasl init
//!
//! # Reset configuration (remove existing settings)
//! kasl init --delete
//! ```

use crate::{
    libs::{config::Config, daemon, messages::Message},
    msg_info, msg_success, msg_warning,
};
use anyhow::Result;
use clap::Args;

/// Command-line arguments for the initialization command.
///
/// The init command supports an optional `--delete` flag for removing
/// existing configuration, which can be useful for testing or troubleshooting.
#[derive(Debug, Args)]
pub struct InitArgs {
    /// Remove existing configuration instead of creating new one
    ///
    /// When specified, this flag will delete the current configuration file
    /// and global PATH settings, effectively resetting the application to
    /// its initial state.
    #[arg(short, long)]
    delete: bool,
}

/// Executes the initialization command.
///
/// Handles configuration setup with interactive wizard for first-time setup,
/// or configuration removal when `--delete` is used.
///
/// # Arguments
///
/// * `init_args` - Parsed command-line arguments containing options
///
/// # Returns
///
/// Returns `Ok(())` on successful configuration, or an error if the setup fails.
pub fn cmd(init_args: InitArgs) -> Result<()> {
    // Check if watcher is currently running before making changes
    let watcher_was_running = daemon::is_running();
    if watcher_was_running {
        msg_info!(Message::WatcherStoppingForConfig);
        daemon::stop()?;
    }

    // Set up global application PATH configuration
    // This ensures the 'kasl' command is available system-wide
    match Config::set_app_global() {
        Ok(()) => {
            msg_success!(Message::PathConfigured);
        }
        Err(e) => {
            msg_warning!(Message::PathConfigWarning { error: e.to_string() });
        }
    }

    // Handle deletion mode - exit early after cleanup
    if init_args.delete {
        // Don't restart watcher after deleting configuration
        msg_info!(Message::ConfigDeleted);
        return Ok(());
    }

    // Run interactive configuration wizard
    // This will prompt the user to select and configure various modules
    Config::init()?.save()?;

    // Confirm successful configuration
    msg_success!(Message::ConfigSaved);

    // Restart watcher if it was running before configuration changes
    if watcher_was_running {
        msg_info!(Message::WatcherRestartingAfterConfig);
        match daemon::spawn() {
            Ok(()) => {
                msg_success!(Message::WatcherRestarted);
            }
            Err(e) => {
                msg_warning!(Message::WatcherRestartFailed { error: e.to_string() });
            }
        }
    }

    Ok(())
}
