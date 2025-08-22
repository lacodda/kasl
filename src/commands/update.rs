//! Application self-update command.
//!
//! This command handles checking for and installing newer versions of kasl
//! from GitHub releases. It provides automatic binary replacement with
//! backup and rollback capabilities.

use crate::{
    libs::{daemon, messages::Message, update::Updater},
    msg_info, msg_success,
};
use anyhow::Result;

/// Executes the application update process.
///
/// Performs a complete update workflow including version check, platform detection,
/// download, extraction, and safe replacement of the current executable.
///
/// # Returns
///
/// Returns `Ok(())` on successful update or if no update is needed.
/// Returns an error if the update process fails.
pub async fn cmd() -> Result<()> {
    // Check if watcher is currently running before update
    let watcher_was_running = daemon::is_running();
    
    if watcher_was_running {
        msg_info!(Message::WatcherStoppingForUpdate);
        daemon::stop()?;
    }

    // Create a new Updater instance with GitHub API configuration
    let mut updater = Updater::new()?;

    // Check GitHub API for the latest release version
    let needs_update = updater.check_for_latest_release().await?;

    if !needs_update {
        msg_info!(Message::NoUpdateRequired);
        
        // If watcher was running before update check, restart it
        if watcher_was_running {
            msg_info!(Message::WatcherRestartingAfterUpdate);
            daemon::spawn()?;
        }
        
        return Ok(());
    }

    // Download and install the latest version
    // This includes downloading the archive, extracting the binary,
    // backing up the current executable, and replacing it
    updater.perform_update().await?;

    // Restart watcher if it was running before the update
    if watcher_was_running {
        msg_info!(Message::WatcherRestartingAfterUpdate);
        daemon::spawn()?;
    }

    // Confirm successful update with version information
    msg_success!(Message::UpdateCompleted {
        app_name: updater.name,
        version: updater.latest_version.as_deref().unwrap_or("unknown").to_string()
    });

    Ok(())
}
