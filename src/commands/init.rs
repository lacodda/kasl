//! Application configuration initialization command.
//!
//! Provides an interactive setup wizard that guides users through configuring kasl for first-time use.
//!
//! ## Usage
//!
//! ```bash
//! # Run interactive setup wizard
//! kasl setup
//!
//! # Reset configuration (remove existing settings)
//! kasl setup --delete
//! ```

use crate::{
    libs::{config::Config, daemon, messages::Message},
    msg_info, msg_success, msg_warning,
};
use anyhow::Result;
use clap::Args;

/// Command-line arguments for the setup command.
///
/// The setup command supports an optional `--delete` flag for removing
/// existing configuration, which can be useful for testing or troubleshooting.
#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Delete the config file instead of running the wizard
    ///
    /// Every setting goes back to its default. The database, the OS keyring
    /// and the PATH are left as they are.
    #[arg(short, long)]
    delete: bool,
}

/// Executes the setup command.
///
/// Handles configuration setup with interactive wizard for first-time setup,
/// or configuration removal when `--delete` is used.
///
pub fn cmd(setup_args: SetupArgs) -> Result<()> {
    // Refused before the watcher is touched: the wizard failing on the
    // missing terminal after the stop below left a script's run with no
    // watcher at all.
    if !setup_args.delete {
        crate::libs::prompt::ensure_interactive(
            "`kasl setup` is an interactive wizard and needs a terminal; `kasl config set KEY VALUE` changes one setting without one",
        )?;
    }

    // Check if watcher is currently running before making changes
    let watcher_was_running = daemon::is_running();
    if watcher_was_running {
        msg_info!(Message::WatcherStoppingForConfig);
        daemon::stop()?;
    }

    if setup_args.delete {
        // Until 1.21 this said "deleted" and deleted nothing - and put kasl
        // on the PATH on its way out. It removes the file now, and only the
        // file: the database and the keyring are not configuration.
        let path = Config::path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
            msg_success!(Message::ConfigDeleted(path.display().to_string()));
        } else {
            msg_info!(Message::ConfigNothingToDelete(path.display().to_string()));
        }
    } else {
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

        // Run interactive configuration wizard
        // This will prompt the user to select and configure various modules
        Config::init()?.save()?;

        // Confirm successful configuration
        msg_success!(Message::ConfigSaved);
    }

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
