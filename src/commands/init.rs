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
    libs::{config::Config, messages::Message},
    msg_success,
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
    // Set up global application PATH configuration
    // This ensures the 'kasl' command is available system-wide
    let _ = Config::set_app_global();

    // Handle deletion mode - exit early after cleanup
    if init_args.delete {
        return Ok(());
    }

    // Run interactive configuration wizard
    // This will prompt the user to select and configure various modules
    Config::init()?.save()?;

    // Confirm successful configuration
    msg_success!(Message::ConfigSaved);
    Ok(())
}
