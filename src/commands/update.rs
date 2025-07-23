use crate::{
    libs::{messages::Message, update::Updater},
    msg_info, msg_success,
};
use anyhow::Result;

/// Executes the application update process.
///
/// This function initializes the `Updater`, checks for a new release,
/// and performs the update if a newer version is available.
pub async fn cmd() -> Result<()> {
    // Create a new Updater instance.
    let mut updater = Updater::new()?;

    // Check for the latest release on GitHub.
    let needs_update = updater.check_for_latest_release().await?;

    if !needs_update {
        msg_info!(Message::NoUpdateRequired);
        return Ok(());
    }

    // If an update is available, perform the update.
    updater.perform_update().await?;

    msg_success!(Message::UpdateCompleted {
        app_name: updater.name,
        version: updater.latest_version.as_deref().unwrap_or("unknown").to_string()
    });

    Ok(())
}
