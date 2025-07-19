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
use std::time::Duration;

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
/// If a daemon is already running, it will be stopped first.
fn spawn_daemon() -> Result<(), Box<dyn Error>> {
    let pid_path = DataStorage::new().get_path("kasl-watch.pid")?;

    // Check if a daemon is already running and stop it
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            println!("Stopping existing watcher (PID: {})...", pid_str.trim());
            // Try to stop the existing daemon
            if let Err(e) = stop_daemon_internal() {
                eprintln!("Warning: Failed to stop existing daemon: {}", e);
                // Remove the PID file anyway in case the process is already dead
                let _ = std::fs::remove_file(&pid_path);
            }
            // Give the old process time to clean up
            std::thread::sleep(Duration::from_millis(500));
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
        let pid = child.id();
        std::fs::write(pid_path, pid.to_string())?;
        println!("Watcher started in the background (PID: {}).", pid);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let child = std::process::Command::new(current_exe)
            .arg("--daemon-run")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
        let pid = child.id();
        std::fs::write(pid_path, pid.to_string())?;
        println!("Watcher started in the background (PID: {}).", pid);
    }

    #[cfg(not(any(unix, windows)))]
    {
        return Err("Daemon mode is not supported on this platform.".into());
    }

    Ok(())
}

/// Finds and stops the running daemon process.
fn stop_daemon() -> Result<(), Box<dyn Error>> {
    match stop_daemon_internal() {
        Ok(()) => Ok(()),
        Err(e) => {
            // If the daemon wasn't running, that's okay
            if e.to_string().contains("not found") || e.to_string().contains("not running") {
                println!("Watcher is not running.");
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Internal function to stop the daemon, used by both stop_daemon and spawn_daemon.
fn stop_daemon_internal() -> Result<(), Box<dyn Error>> {
    let pid_path = DataStorage::new().get_path("kasl-watch.pid")?;
    if !pid_path.exists() {
        return Err("Watcher does not appear to be running (PID file not found).".into());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: usize = pid_str.trim().parse().map_err(|_| "Invalid PID file content")?;

    use sysinfo::{Pid, System};
    let s = System::new_with_specifics(sysinfo::RefreshKind::new().with_processes(sysinfo::ProcessRefreshKind::new()));

    let mut process_found = false;
    let mut kill_successful = false;

    if let Some(process) = s.process(Pid::from(pid)) {
        process_found = true;
        // Check if it's actually our kasl process
        if let Some(exe) = process.exe() {
            if exe.to_string_lossy().contains("kasl") {
                if process.kill() {
                    kill_successful = true;
                    println!("Watcher process (PID: {}) stopped successfully.", pid);
                } else {
                    eprintln!("Failed to stop watcher process (PID: {}).", pid);
                }
            } else {
                eprintln!("PID {} does not appear to be a kasl process.", pid);
            }
        }
    }

    // Clean up the PID file regardless of whether the process was found.
    std::fs::remove_file(pid_path)?;

    if !process_found {
        return Err(format!("Watcher process (PID: {}) not found. It may have already stopped.", pid).into());
    }

    if !kill_successful && process_found {
        return Err(format!("Failed to stop watcher process (PID: {})", pid).into());
    }

    Ok(())
}
