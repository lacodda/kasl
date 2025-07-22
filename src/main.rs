use anyhow::Result;
use kasl::commands::Cli;
use kasl::libs::update::Updater;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--daemon-run" {
        kasl::commands::watch::run_as_daemon().await?;
    } else {
        Updater::show_update_notification().await;
        Cli::menu().await?;
    }
    Ok(())
}
