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
/// This function performs a complete update workflow:
/// 1. **Version Check**: Queries GitHub API for the latest release
/// 2. **Platform Detection**: Identifies the correct binary for the current OS/architecture
/// 3. **Download**: Retrieves the latest release archive
/// 4. **Extraction**: Unpacks the new binary from the archive
/// 5. **Replacement**: Safely replaces the current executable with backup
///
/// The update process is designed to be safe and atomic:
/// - Creates backups of the current executable before replacement
/// - Validates downloaded archives before extraction
/// - Provides clear feedback about the update process
/// - Handles network errors gracefully
///
/// # Update Sources
///
/// Updates are fetched from GitHub releases at:
/// `https://github.com/{owner}/{repo}/releases/latest`
///
/// The updater automatically selects the appropriate asset based on:
/// - **Architecture**: x86_64, aarch64, etc.
/// - **Operating System**: Windows (MSVC), macOS (Darwin), Linux (musl)
///
/// # Examples
///
/// ```bash
/// # Check for and install updates
/// kasl update
/// ```
///
/// # Platform Support
///
/// Supported platform identifiers:
/// - `x86_64-pc-windows-msvc` - Windows 64-bit
/// - `x86_64-apple-darwin` - macOS Intel
/// - `aarch64-apple-darwin` - macOS Apple Silicon
/// - `x86_64-unknown-linux-musl` - Linux 64-bit
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
