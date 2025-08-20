//! Application self-update command.
//!
//! This command handles checking for and installing newer versions of kasl
//! from GitHub releases. It provides automatic binary replacement with
//! backup and rollback capabilities.

use crate::{
    libs::{messages::Message, update::Updater},
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
    // Create a new Updater instance with GitHub API configuration
    let mut updater = Updater::new()?;

    // Check GitHub API for the latest release version
    let needs_update = updater.check_for_latest_release().await?;

    if !needs_update {
        msg_info!(Message::NoUpdateRequired);
        return Ok(());
    }

    // Download and install the latest version
    // This includes downloading the archive, extracting the binary,
    // backing up the current executable, and replacing it
    updater.perform_update().await?;

    // Confirm successful update with version information
    msg_success!(Message::UpdateCompleted {
        app_name: updater.name,
        version: updater.latest_version.as_deref().unwrap_or("unknown").to_string()
    });

    Ok(())
}
