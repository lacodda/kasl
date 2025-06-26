use kasl::commands::Cli;
use kasl::libs::update::Updater;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // This logic checks for a hidden flag used to launch the daemon process.
    // If the flag is present, it runs the watcher directly and bypasses the normal CLI.
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--daemon-run" {
        kasl::commands::watch::run_as_daemon().await?;
    } else {
        // Normal application flow
        Updater::show_update_notification().await;
        Cli::menu().await?;
    }
    Ok(())
}
