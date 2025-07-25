use anyhow::Result;
use kasl::commands::Cli;
use kasl::libs::update::Updater;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing only if debug mode is enabled
    if env::var("KASL_DEBUG").is_ok() || env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "kasl=debug".into()))
            .init();
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--daemon-run" {
        kasl::commands::watch::run_as_daemon().await?;
    } else {
        Updater::show_update_notification().await;
        Cli::menu().await?;
    }
    Ok(())
}
