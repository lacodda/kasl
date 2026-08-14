//! # Kasl - Key Activity Synchronization and Logging
//!
//! A command-line utility for tracking work activities, managing tasks,
//! and generating productivity reports.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::commands::Cli;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     Cli::menu().await
//! }
//! ```

pub mod api;
pub mod commands;
pub mod db;
pub mod libs;

/// Runs the CLI: the shared entry point behind both the `kasl` and `ka` binaries.
///
/// Sets up tracing, handles the internal `--daemon-run` mode used when the
/// watcher spawns itself, and otherwise dispatches the parsed command.
///
/// # Errors
///
/// Propagates whatever the executed command fails with.
pub fn run() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        // Initialize tracing only if debug mode is enabled; otherwise log output
        // would clutter normal CLI usage.
        if std::env::var("KASL_DEBUG").is_ok() || std::env::var("RUST_LOG").is_ok() {
            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "kasl=debug".into()))
                .init();
        }

        // Intercepted before clap: this flag is how the background monitoring
        // process is launched, not part of the public command surface.
        let args: Vec<String> = std::env::args().collect();
        if args.len() > 1 && args[1] == "--daemon-run" {
            commands::watch::run_as_daemon().await?;
        } else {
            // Non-blocking; only surfaces a notification when one is due.
            libs::update::Updater::show_update_notification().await;

            commands::Cli::menu().await?;
        }

        Ok(())
    })
}
