use crate::libs::update::Updater;
use std::error::Error;

/// Executes the application update process.
///
/// This function initializes the `Updater`, checks for a new release,
/// and performs the update if a newer version is available.
pub async fn cmd() -> Result<(), Box<dyn Error>> {
    // Create a new Updater instance.
    let mut updater = Updater::new()?;

    // Check for the latest release on GitHub.
    let needs_update = updater.check_for_latest_release().await?;

    if !needs_update {
        println!("No update required. You are using the latest version!");
        return Ok(());
    }

    // If an update is available, perform the update.
    updater.perform_update().await?;

    println!(
        "The {} application has been successfully updated to version {}!",
        updater.name,
        updater.latest_version.as_deref().unwrap_or("unknown")
    );

    Ok(())
}
