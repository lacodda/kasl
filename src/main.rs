use kasl::commands::Cli;
use kasl::libs::update::Updater;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // This logic checks for a hidden flag used to launch the daemon process.
    // If the flag is present, it runs the watcher directly and bypasses the normal CLI.
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--daemon-run" {
        run_daemon_with_signal_handling().await?;
    } else {
        // Normal application flow
        Updater::show_update_notification().await;
        Cli::menu().await?;
    }
    Ok(())
}

/// Runs the daemon with proper signal handling for graceful shutdown.
async fn run_daemon_with_signal_handling() -> Result<(), Box<dyn Error>> {
    // Set up a channel to handle shutdown signals
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Spawn the signal handler in a separate task
    #[cfg(unix)]
    {
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm = signal(SignalKind::terminate()).expect("Failed to create SIGTERM handler");
            let mut sigint = signal(SignalKind::interrupt()).expect("Failed to create SIGINT handler");

            tokio::select! {
                _ = sigterm.recv() => {
                    println!("Received SIGTERM, shutting down gracefully...");
                }
                _ = sigint.recv() => {
                    println!("Received SIGINT, shutting down gracefully...");
                }
            }

            let _ = shutdown_tx.send(());
        });
    }

    #[cfg(windows)]
    {
        tokio::spawn(async move {
            match tokio::signal::ctrl_c().await {
                Ok(()) => {
                    println!("Received Ctrl+C, shutting down gracefully...");
                }
                Err(e) => {
                    eprintln!("Failed to listen for Ctrl+C: {}", e);
                }
            }

            let _ = shutdown_tx.send(());
        });
    }

    #[cfg(not(any(unix, windows)))]
    {
        // For other platforms, just run without signal handling
        eprintln!("Warning: Signal handling not supported on this platform");
    }

    // Run the monitor in a separate task
    let monitor_handle = tokio::spawn(async move {
        match kasl::commands::watch::run_as_daemon().await {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("Monitor error: {}", e);
                Err(format!("Monitor error: {}", e))
            }
        }
    });

    // Wait for either the monitor to finish or a shutdown signal
    tokio::select! {
        result = monitor_handle => {
            match result {
                Ok(Ok(())) => println!("Monitor exited normally"),
                Ok(Err(e)) => eprintln!("{}", e),
                Err(e) => eprintln!("Monitor task panicked: {}", e),
            }
        }
        _ = shutdown_rx => {
            println!("Shutting down monitor...");
            // The monitor will be dropped when this function exits
        }
    }

    // Clean up PID file on exit
    use kasl::libs::data_storage::DataStorage;
    let pid_path = DataStorage::new().get_path("kasl-watch.pid")?;
    if pid_path.exists() {
        let _ = std::fs::remove_file(&pid_path);
    }

    Ok(())
}
