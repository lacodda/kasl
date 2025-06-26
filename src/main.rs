use kasl::commands::Cli;
use kasl::libs::update::Updater;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Updater::show_update_notification().await;
    Cli::menu().await
}
