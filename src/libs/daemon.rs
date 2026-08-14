//! Background process management for `kasl watch`: spawn detached,
//! track by PID file, stop, and the daemon's own signal-handled entry
//! point.
//!
//! ```rust,no_run
//! # async fn f() -> anyhow::Result<()> {
//! use kasl::libs::daemon;
//!
//! daemon::spawn()?;                           // Start background monitoring
//! daemon::stop()?;                            // Stop background monitoring
//! daemon::run_with_signal_handling().await?;  // Run with signal handling
//! # Ok(())
//! # }
//! ```

use crate::libs::config::Config;
use crate::libs::data_storage::DataStorage;
use crate::libs::messages::Message;
use crate::libs::monitor::Monitor;
use crate::{msg_bail_anyhow, msg_error, msg_error_anyhow, msg_info, msg_warning};
use anyhow::Result;
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

/// PID file in the app data directory; written on spawn, removed on
/// shutdown, and the single source of "is a daemon running".
const PID_FILE: &str = "kasl-watch.pid";

/// The daemon entry point: runs the monitor and the Jira inbox poller,
/// shutting both down on SIGTERM/SIGINT (Ctrl+C on Windows) and removing
/// the PID file on the way out.
#[instrument]
pub async fn run_with_signal_handling() -> Result<()> {
    info!("Starting daemon with signal handling");

    // Set up a channel to handle shutdown signals
    // This allows coordinated shutdown between signal handlers and the monitor
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Spawn the signal handler in a separate task
    // This ensures signal handling doesn't block the main monitoring loop
    #[cfg(unix)]
    {
        tokio::spawn(async move {
            use tokio::signal::unix::{SignalKind, signal};

            // Set up handlers for standard Unix termination signals
            let mut sigterm = signal(SignalKind::terminate()).unwrap_or_else(|_| panic!("{}", Message::FailedToCreateSigtermHandler));
            let mut sigint = signal(SignalKind::interrupt()).unwrap_or_else(|_| panic!("{}", Message::FailedToCreateSigintHandler));

            // Wait for any termination signal
            tokio::select! {
                _ = sigterm.recv() => {
                    msg_info!(Message::WatcherReceivedSigterm);
                }
                _ = sigint.recv() => {
                    msg_info!(Message::WatcherReceivedSigint);
                }
            }

            // Signal the main loop to shut down gracefully
            let _ = shutdown_tx.send(());
        });
    }

    #[cfg(windows)]
    {
        tokio::spawn(async move {
            // Handle Windows console events
            match tokio::signal::ctrl_c().await {
                Ok(()) => {
                    msg_info!(Message::WatcherReceivedCtrlC);
                }
                Err(e) => {
                    msg_error!(Message::WatcherCtrlCListenFailed(e.to_string()));
                }
            }

            // Signal the main loop to shut down gracefully
            let _ = shutdown_tx.send(());
        });
    }

    #[cfg(not(any(unix, windows)))]
    {
        // For other platforms, just run without signal handling
        // This ensures the application still works on unsupported platforms
        msg_warning!(Message::WatcherSignalHandlingNotSupported);
    }

    // Run the monitor in a separate task
    // This allows concurrent execution with signal handling
    let monitor_handle = tokio::spawn(async move {
        match run_monitor().await {
            Ok(()) => Ok(()),
            Err(e) => Err(Message::MonitorError(e.to_string())),
        }
    });

    // Poll Jira inbox in a sibling task (independent cadence from activity monitor)
    let inbox_handle = tokio::spawn(async move {
        crate::libs::jira_inbox::run_poller().await;
    });

    // Wait for either the monitor to finish or a shutdown signal
    // This provides coordinated shutdown between different components
    tokio::select! {
        result = monitor_handle => {
            // Monitor task completed (either successfully or with error)
            inbox_handle.abort();
            match result {
                Ok(Ok(())) => msg_info!(Message::MonitorExitedNormally),
                Ok(Err(e)) => msg_error!(Message::MonitorError(e.to_string())),
                Err(e) => msg_error!(Message::MonitorTaskPanicked(e.to_string())),
            }
        }
        _ = shutdown_rx => {
            // Received shutdown signal
            inbox_handle.abort();
            msg_info!(Message::MonitorShuttingDown);
            // The monitor will be dropped when this function exits
        }
    }

    // Clean up PID file on exit
    // This ensures the PID file doesn't become stale
    let pid_path = DataStorage::new().get_path(PID_FILE)?;
    if pid_path.exists() {
        let _ = std::fs::remove_file(&pid_path);
    }

    Ok(())
}

/// Loads config (defaults for missing sections) and runs the monitor loop.
async fn run_monitor() -> Result<()> {
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();

    let mut monitor = Monitor::new(monitor_config)?;
    monitor.run().await
}

/// Re-launches the current executable detached (`--daemon-run`), first
/// stopping any daemon the PID file points at, and records the new PID.
/// Detachment is `setsid` on Unix, `CREATE_NO_WINDOW` on Windows; a
/// failed stop of the old daemon is a warning, not a blocker.
///
/// ```rust,no_run
/// # fn main() -> anyhow::Result<()> {
/// use kasl::libs::daemon;
///
/// // Start background monitoring
/// daemon::spawn()?;
/// println!("Background monitoring started");
/// # Ok(())
/// # }
/// ```
#[instrument]
pub fn spawn() -> Result<()> {
    debug!("Attempting to spawn daemon process");
    let pid_path = DataStorage::new().get_path(PID_FILE)?;

    // Check if a daemon is already running and stop it
    // This ensures only one daemon instance is active at a time
    if pid_path.exists()
        && let Ok(pid_str) = std::fs::read_to_string(&pid_path)
    {
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

    // Get the current executable path for spawning
    let current_exe = std::env::current_exe().unwrap_or_else(|_| panic!("{}", Message::FailedToGetCurrentExecutable.to_string()));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // Spawn daemon process with session detachment
        let mut command = std::process::Command::new(current_exe);
        command.arg("--daemon-run");
        // SAFETY: setsid is async-signal-safe and touches no shared state,
        // which is all pre_exec requires between fork and exec.
        unsafe {
            command.pre_exec(|| {
                // Detach from the current session to become a daemon
                // This ensures the process continues running after parent exits
                nix::unistd::setsid()?;
                Ok(())
            });
        }
        let child = command.spawn()?;

        let pid = child.id();
        std::fs::write(pid_path, pid.to_string())?;
        msg_info!(Message::WatcherStarted(pid));
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        // Windows-specific flags for background process creation
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Spawn daemon process without console window
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
        // Platform not supported for daemon mode
        msg_bail_anyhow!(Message::DaemonModeNotSupported);
    }

    Ok(())
}

/// True when the PID file exists, parses, and names a live process.
pub fn is_running() -> bool {
    let pid_path = match DataStorage::new().get_path(PID_FILE) {
        Ok(path) => path,
        Err(_) => return false,
    };

    // Check if PID file exists
    if !pid_path.exists() {
        return false;
    }

    // Read and parse the PID from the file
    let pid_str = match std::fs::read_to_string(&pid_path) {
        Ok(content) => content,
        Err(_) => return false,
    };

    let pid: u32 = match pid_str.trim().parse() {
        Ok(pid) => pid,
        Err(_) => return false,
    };

    // Check if process is actually running
    is_process_running(pid)
}

/// Platform-specific "does this PID exist" probe.
fn is_process_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use winapi::um::errhandlingapi::GetLastError;
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processthreadsapi::OpenProcess;
        use winapi::um::winnt::PROCESS_QUERY_INFORMATION;

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
            if handle.is_null() {
                let error = GetLastError();
                // ERROR_INVALID_PARAMETER (87) means process doesn't exist
                return error != 87;
            }
            CloseHandle(handle);
            true
        }
    }

    #[cfg(unix)]
    {
        use std::process::Command;

        // Use ps command to check if process exists
        match Command::new("ps").arg("-p").arg(pid.to_string()).output() {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        // For unsupported platforms, assume not running
        false
    }
}

/// Stops the daemon; "already stopped" counts as success, so cleanup
/// scripts can call it unconditionally.
///
/// ```rust,no_run
/// # fn main() -> anyhow::Result<()> {
/// use kasl::libs::daemon;
///
/// daemon::stop()?;
/// println!("Monitoring stopped");
/// # Ok(())
/// # }
/// ```
pub fn stop() -> Result<()> {
    match stop_internal() {
        Ok(()) => Ok(()),
        Err(e) => {
            // If the daemon wasn't running, that's okay
            // This provides a better user experience than reporting errors
            if e.to_string().contains("not found") || e.to_string().contains("not running") {
                msg_info!(Message::WatcherNotRunning);
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

/// Termination with precise errors, shared by [`stop`] and [`spawn`];
/// the PID file is removed even when the process is already gone.
fn stop_internal() -> Result<()> {
    let pid_path = DataStorage::new().get_path(PID_FILE)?;

    // The daemon removes its own PID file on shutdown, so every file
    // operation below can race with a dying daemon: a file that has
    // disappeared at any step means the watcher is already stopped.
    let pid_str = match std::fs::read_to_string(&pid_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            msg_bail_anyhow!(Message::WatcherNotRunningPidNotFound);
        }
        Err(e) => return Err(e.into()),
    };
    let pid: u32 = pid_str.trim().parse().map_err(|_| msg_error_anyhow!(Message::InvalidPidFileContent))?;

    // Attempt to terminate the process
    let killed = kill_process(pid)?;

    // Clean up the PID file regardless of whether the process was found
    // This prevents stale PID files from interfering with future operations
    if let Err(e) = std::fs::remove_file(&pid_path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(e.into());
    }

    if killed {
        msg_info!(Message::WatcherStopped(pid));
    } else {
        // The process was already gone; removing the stale PID file is all
        // that stopping requires, so this is a success, not an error.
        msg_info!(Message::WatcherNotRunning);
    }
    Ok(())
}

/// Terminates the process via `TerminateProcess` - Windows has no
/// SIGTERM equivalent, so forceful is the reliable option. Returns
/// `Ok(false)` when the process does not exist.
#[cfg(windows)]
fn kill_process(pid: u32) -> Result<bool> {
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
    use winapi::um::winnt::PROCESS_TERMINATE;

    unsafe {
        // Open a handle to the target process with termination rights
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            let error = GetLastError();
            if error == 87 {
                // ERROR_INVALID_PARAMETER - process doesn't exist
                return Ok(false);
            }
            msg_bail_anyhow!(Message::FailedToOpenProcess(error));
        }

        // Attempt to terminate the process
        let result = TerminateProcess(handle, 0);

        // Always close the handle to prevent resource leaks
        CloseHandle(handle);

        if result == 0 {
            // Termination failed - get error details
            let error = GetLastError();
            msg_bail_anyhow!(Message::FailedToTerminateProcess(error));
        } else {
            // Give the process time to actually terminate
            std::thread::sleep(Duration::from_millis(100));
            Ok(true)
        }
    }
}

/// SIGTERM first, up to a second of grace, then SIGKILL; uses `ps` and
/// `kill` rather than raw syscalls. Returns `Ok(false)` when the process
/// does not exist.
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
            // Process terminated gracefully
            return Ok(true);
        }
    }

    // Process didn't terminate gracefully, force kill
    Command::new("kill").arg("-9").arg(pid.to_string()).output()?;

    // Give a brief moment for forced termination
    std::thread::sleep(Duration::from_millis(100));
    Ok(true)
}

#[cfg(not(any(unix, windows)))]
fn kill_process(_pid: u32) -> Result<bool> {
    msg_bail_anyhow!(Message::ProcessTerminationNotSupported);
}
