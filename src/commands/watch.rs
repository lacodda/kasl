//! Contains the logic for the `watch` command.
//!
//! This module handles starting, stopping, and running the activity monitor
//! both as a foreground process for debugging and as a background daemon
//! for normal use.

use crate::libs::config::Config;
use crate::libs::data_storage::DataStorage;
use crate::libs::monitor::Monitor;
use clap::Args;
use std::error::Error;

/// Command-line arguments for the `watch` command.
#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Runs the watcher in the foreground for debugging instead of as a background process.
    #[arg(long)]
    foreground: bool,

    /// Stops any running background watcher process.
    #[arg(long, short)]
    stop: bool,
}

/// Main entry point for the `watch` command, acting as a dispatcher.
pub async fn cmd(args: WatchArgs) -> Result<(), Box<dyn Error>> {
    if args.stop {
        stop_daemon()?;
    } else if args.foreground {
        println!("Starting watcher in foreground... Press Ctrl+C to exit.");
        run_monitor().await?;
    } else {
        spawn_daemon()?;
    }
    Ok(())
}

/// The core logic that initializes and runs the activity monitor.
/// This function is called either directly for foreground mode or by the daemon process.
async fn run_monitor() -> Result<(), Box<dyn Error>> {
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();
    let mut monitor = Monitor::new(monitor_config)?;
    monitor.run().await
}

/// This function is executed by the detached daemon process.
/// It's called from `main.rs` when the `--daemon-run` argument is detected.
pub async fn run_as_daemon() -> Result<(), Box<dyn Error>> {
    // In a future update, stdout and stderr could be redirected to log files here.
    // For now, the daemon runs without console output.
    run_monitor().await
}

/// Spawns the application as a detached background process.
fn spawn_daemon() -> Result<(), Box<dyn Error>> {
    let pid_path = DataStorage::new().get_path("kasl-watch.pid")?;
    // Check if the PID file exists to prevent running multiple instances.
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            println!("Watcher appears to be running with PID {}. Use `kasl watch --stop` to stop it.", pid_str);
            println!("If it's not running, please remove the file: {}", pid_path.display());
            return Ok(());
        }
    }

    let current_exe = std::env::current_exe().expect("Failed to get the path of the current executable");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let child = std::process::Command::new(current_exe)
            .arg("--daemon-run")
            .before_exec(|| {
                // Detach from the current session to become a daemon.
                nix::unistd::setsid()?;
                Ok(())
            })
            .spawn()?;
        std::fs::write(pid_path, child.id().to_string())?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let child = std::process::Command::new(current_exe)
            .arg("--daemon-run")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
        std::fs::write(pid_path, child.id().to_string())?;
    }

    #[cfg(not(any(unix, windows)))]
    {
        return Err("Daemon mode is not supported on this platform.".into());
    }

    println!("Watcher started in the background.");
    Ok(())
}

/// Finds and stops the running daemon process.
fn stop_daemon() -> Result<(), Box<dyn Error>> {
    let pid_path = DataStorage::new().get_path("kasl-watch.pid")?;
    if !pid_path.exists() {
        println!("Watcher does not appear to be running (PID file not found).");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: usize = pid_str.trim().parse().map_err(|_| "Invalid PID file content")?;

    use sysinfo::{Pid, System};
    let s = System::new_with_specifics(sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::new()));

    if let Some(process) = s.process(Pid::from(pid)) {
        if process.kill() {
            println!("Watcher process (PID: {}) stopped successfully.", pid);
        } else {
            eprintln!("Failed to stop watcher process (PID: {}).", pid);
        }
    } else {
        println!("Watcher process (PID: {}) not found. It may have already stopped.", pid);
    }

    // Clean up the PID file regardless of whether the process was found.
    std::fs::remove_file(pid_path)?;
    Ok(())
}
