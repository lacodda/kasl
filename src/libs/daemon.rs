//! Daemon management functionality for the watch command.
//!
//! This module handles the lifecycle of the background process including
//! starting, stopping, and signal handling.

use crate::libs::config::Config;
use crate::libs::data_storage::DataStorage;
use crate::libs::messages::Message;
use crate::libs::monitor::Monitor;
use crate::{msg_bail_anyhow, msg_error, msg_error_anyhow, msg_info, msg_warning};
use anyhow::Result;
use std::time::Duration;

const PID_FILE: &str = "kasl-watch.pid";

/// Runs the daemon with proper signal handling for graceful shutdown.
pub async fn run_with_signal_handling() -> Result<()> {
    // Set up a channel to handle shutdown signals
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Spawn the signal handler in a separate task
    #[cfg(unix)]
    {
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm = signal(SignalKind::terminate()).expect(&Message::FailedToCreateSigtermHandler.to_string());
            let mut sigint = signal(SignalKind::interrupt()).expect(&Message::FailedToCreateSigintHandler.to_string());

            tokio::select! {
                _ = sigterm.recv() => {
                    msg_info!(Message::WatcherReceivedSigterm);
                }
                _ = sigint.recv() => {
                    msg_info!(Message::WatcherReceivedSigint);
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
                    msg_info!(Message::WatcherReceivedCtrlC);
                }
                Err(e) => {
                    msg_error!(Message::WatcherCtrlCListenFailed(e.to_string()));
                }
            }

            let _ = shutdown_tx.send(());
        });
    }

    #[cfg(not(any(unix, windows)))]
    {
        // For other platforms, just run without signal handling
        msg_warning!(Message::WatcherSignalHandlingNotSupported);
    }

    // Run the monitor in a separate task
    let monitor_handle = tokio::spawn(async move {
        match run_monitor().await {
            Ok(()) => Ok(()),
            Err(e) => Err(Message::MonitorError(e.to_string())),
        }
    });

    // Wait for either the monitor to finish or a shutdown signal
    tokio::select! {
        result = monitor_handle => {
            match result {
                Ok(Ok(())) => msg_info!(Message::MonitorExitedNormally),
                Ok(Err(e)) => msg_error!(Message::MonitorError(e.to_string())),
                Err(e) => msg_error!(Message::MonitorTaskPanicked(e.to_string())),
            }
        }
        _ = shutdown_rx => {
            msg_info!(Message::MonitorShuttingDown);
            // The monitor will be dropped when this function exits
        }
    }

    // Clean up PID file on exit
    let pid_path = DataStorage::new().get_path(PID_FILE)?;
    if pid_path.exists() {
        let _ = std::fs::remove_file(&pid_path);
    }

    Ok(())
}

/// The core logic that initializes and runs the activity monitor.
async fn run_monitor() -> Result<()> {
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();
    let mut monitor = Monitor::new(monitor_config)?;
    monitor.run().await
}

/// Spawns the application as a detached background process.
/// If a daemon is already running, it will be stopped first.
pub fn spawn() -> Result<()> {
    let pid_path = DataStorage::new().get_path(PID_FILE)?;

    // Check if a daemon is already running and stop it
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            msg_info!(Message::WatcherStoppingExisting(pid_str.trim().to_string()));
            // Try to stop the existing daemon
            if let Err(e) = stop_internal() {
                msg_warning!(Message::WatcherFailedToStopExisting(e.to_string()));
                // Remove the PID file anyway in case the process is already dead
                let _ = std::fs::remove_file(&pid_path);
            }
            // Give the old process time to clean up
            std::thread::sleep(Duration::from_millis(1000));
        }
    }

    let current_exe = std::env::current_exe().expect(&Message::FailedToGetCurrentExecutable.to_string());

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
        msg_info!(Message::WatcherStarted(pid));
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
        msg_info!(Message::WatcherStarted(pid));
    }

    #[cfg(not(any(unix, windows)))]
    {
        msg_bail_anyhow!(Message::DaemonModeNotSupported);
    }

    Ok(())
}

/// Finds and stops the running daemon process.
pub fn stop() -> Result<()> {
    match stop_internal() {
        Ok(()) => Ok(()),
        Err(e) => {
            // If the daemon wasn't running, that's okay
            if e.to_string().contains("not found") || e.to_string().contains("not running") {
                msg_info!(Message::WatcherNotRunning);
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Internal function to stop the daemon, used by both stop and spawn.
fn stop_internal() -> Result<()> {
    let pid_path = DataStorage::new().get_path(PID_FILE)?;
    if !pid_path.exists() {
        msg_bail_anyhow!(Message::WatcherNotRunningPidNotFound);
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str.trim().parse().map_err(|_| msg_error_anyhow!(Message::InvalidPidFileContent))?;

    let killed = kill_process(pid)?;

    // Clean up the PID file regardless of whether the process was found.
    std::fs::remove_file(pid_path)?;

    if killed {
        msg_info!(Message::WatcherStopped(pid));
        Ok(())
    } else {
        msg_bail_anyhow!(Message::WatcherFailedToStop(pid));
    }
}

/// Cross-platform process termination
#[cfg(windows)]
fn kill_process(pid: u32) -> Result<bool> {
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
    use winapi::um::winnt::PROCESS_TERMINATE;

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            let error = GetLastError();
            if error == 87 {
                // ERROR_INVALID_PARAMETER - process doesn't exist
                return Ok(false);
            }
            msg_bail_anyhow!(Message::FailedToOpenProcess(error));
        }

        let result = TerminateProcess(handle, 0);
        CloseHandle(handle);

        if result == 0 {
            let error = GetLastError();
            msg_bail_anyhow!(Message::FailedToTerminateProcess(error));
        } else {
            // Give the process time to actually terminate
            std::thread::sleep(Duration::from_millis(100));
            Ok(true)
        }
    }
}

#[cfg(unix)]
fn kill_process(pid: u32) -> Result<bool> {
    use std::process::Command;

    // Check if process exists using ps
    let output = Command::new("ps").arg("-p").arg(pid.to_string()).output()?;

    if !output.status.success() {
        // Process doesn't exist
        return Ok(false);
    }

    // Send SIGTERM for graceful shutdown
    Command::new("kill").arg("-TERM").arg(pid.to_string()).output()?;

    // Give the process time to terminate gracefully
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(100));

        // Check if process still exists
        let check = Command::new("ps").arg("-p").arg(pid.to_string()).output()?;

        if !check.status.success() {
            // Process terminated
            return Ok(true);
        }
    }

    // Process didn't terminate gracefully, force kill
    Command::new("kill").arg("-9").arg(pid.to_string()).output()?;

    std::thread::sleep(Duration::from_millis(100));
    Ok(true)
}

#[cfg(not(any(unix, windows)))]
fn kill_process(_pid: u32) -> Result<bool> {
    msg_bail_anyhow!(Message::ProcessTerminationNotSupported);
}
