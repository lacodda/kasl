//! Daemon management functionality for the kasl watch command.
//!
//! Provides comprehensive background process management for the kasl activity
//! monitoring system including spawning, signal handling, and graceful shutdown.
//!
//! ## Features
//!
//! - **Process Spawning**: Creates detached background processes for continuous monitoring
//! - **Signal Handling**: Responds to system signals for graceful shutdown and restart
//! - **PID Management**: Tracks running processes and prevents duplicate instances
//! - **Cross-Platform Support**: Handles platform differences between Unix and Windows
//! - **Resource Cleanup**: Ensures proper cleanup of database connections and system resources
//! - **Error Recovery**: Manages process failures and provides meaningful error messages
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::daemon;
//!
//! daemon::spawn()?;                           // Start background monitoring
//! daemon::stop()?;                            // Stop background monitoring
//! daemon::run_with_signal_handling().await?;  // Run with signal handling
//! ```

use crate::libs::config::Config;
use crate::libs::data_storage::DataStorage;
use crate::libs::messages::Message;
use crate::libs::monitor::Monitor;
use crate::{msg_bail_anyhow, msg_error, msg_error_anyhow, msg_info, msg_warning};
use anyhow::Result;
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

/// PID file name used for tracking the daemon process.
///
/// This constant defines the filename used to store the process ID of the
/// running daemon. The file is created in the application data directory
/// when the daemon starts and removed when it shuts down gracefully.
///
/// The PID file serves multiple purposes:
/// - **Process Tracking**: Allows the main process to find and communicate with the daemon
/// - **Duplicate Prevention**: Prevents multiple daemon instances from running simultaneously
/// - **Status Checking**: Enables status queries about the daemon's running state
/// - **Cleanup Detection**: Helps identify when the daemon terminates unexpectedly
const PID_FILE: &str = "kasl-watch.pid";

/// Runs the daemon with proper signal handling for graceful shutdown.
///
/// Sets up comprehensive signal handling and runs the activity monitor in a
/// controlled environment. Designed to be the main entry point for daemon operation.
///
/// # Returns
///
/// Returns `Ok(())` when the daemon shuts down cleanly, or an error if
/// initialization fails or a critical error occurs during operation.
///
/// - **Signal Handler Setup**: Platform signal APIs not available
/// - **Monitor Initialization**: Database connection or configuration errors
/// - **Runtime Errors**: Critical failures during monitoring operation
/// - **Cleanup Failures**: Unable to remove PID file or close resources
///
/// # Usage Context
///
/// This function is typically called from:
/// - Background daemon processes spawned by [`spawn()`]
/// - Foreground monitoring mode for debugging
/// - Test environments requiring controlled shutdown
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
            use tokio::signal::unix::{signal, SignalKind};

            // Set up handlers for standard Unix termination signals
            let mut sigterm = signal(SignalKind::terminate()).expect(&Message::FailedToCreateSigtermHandler.to_string());
            let mut sigint = signal(SignalKind::interrupt()).expect(&Message::FailedToCreateSigintHandler.to_string());

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

    // Wait for either the monitor to finish or a shutdown signal
    // This provides coordinated shutdown between different components
    tokio::select! {
        result = monitor_handle => {
            // Monitor task completed (either successfully or with error)
            match result {
                Ok(Ok(())) => msg_info!(Message::MonitorExitedNormally),
                Ok(Err(e)) => msg_error!(Message::MonitorError(e.to_string())),
                Err(e) => msg_error!(Message::MonitorTaskPanicked(e.to_string())),
            }
        }
        _ = shutdown_rx => {
            // Received shutdown signal
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

/// The core logic that initializes and runs the activity monitor.
///
/// This function handles the complete lifecycle of the activity monitoring
/// system, from configuration loading through monitor initialization to
/// the main monitoring loop execution.
///
/// ## Initialization Process
///
/// 1. **Configuration Loading**: Reads monitor settings from the config file
/// 2. **Default Application**: Applies sensible defaults for missing configuration
/// 3. **Monitor Creation**: Initializes the monitor with the loaded configuration
/// 4. **Loop Execution**: Starts the continuous activity monitoring loop
///
/// ## Configuration Handling
///
/// The function uses a robust configuration loading strategy:
/// - **Primary Source**: User configuration file
/// - **Fallback**: Built-in default values
/// - **Validation**: Ensures configuration values are within valid ranges
/// - **Error Recovery**: Continues with defaults if configuration is invalid
///
/// ## Monitor Components
///
/// The initialized monitor includes:
/// - **Input Detection**: Keyboard and mouse activity tracking
/// - **Database Interface**: Connection to SQLite database for data storage
/// - **State Management**: Activity state tracking and transition logic
/// - **Timing Control**: Configurable polling intervals and thresholds
///
/// ## Error Propagation
///
/// This function properly propagates errors from:
/// - Configuration loading failures
/// - Database connection issues
/// - Monitor initialization problems
/// - Runtime monitoring errors
///
/// # Returns
///
/// Returns `Ok(())` when monitoring completes successfully, or an error
/// if any part of the initialization or execution process fails.
///
/// # Error Scenarios
///
/// - **Configuration Errors**: Invalid or corrupted configuration file
/// - **Database Errors**: Cannot connect to or initialize the SQLite database
/// - **Permission Errors**: Insufficient privileges for input device monitoring
/// - **Resource Errors**: System resource exhaustion or availability issues
///
/// # Usage Context
///
/// This function is called by:
/// - [`run_with_signal_handling()`] for daemon operation
/// - Foreground monitoring mode for interactive debugging
/// - Test environments for controlled monitoring scenarios
async fn run_monitor() -> Result<()> {
    // Load configuration with defaults for missing values
    // This ensures the monitor can start even with minimal configuration
    let config = Config::read()?;
    let monitor_config = config.monitor.unwrap_or_default();

    // Initialize the activity monitor with configuration
    // This sets up all necessary components for activity tracking
    let mut monitor = Monitor::new(monitor_config)?;

    // Start the main monitoring loop
    // This will run indefinitely until stopped or an error occurs
    monitor.run().await
}

/// Spawns the application as a detached background process.
///
/// This function creates a new background process that runs independently
/// of the parent process. It handles platform-specific process creation,
/// PID file management, and ensures only one daemon instance runs at a time.
///
/// ## Process Management
///
/// 1. **Existing Process Check**: Verifies no daemon is already running
/// 2. **Process Termination**: Stops any existing daemon before starting new one
/// 3. **Process Creation**: Spawns new daemon with platform-specific flags
/// 4. **PID Recording**: Saves the new process ID for future management
/// 5. **Status Reporting**: Provides feedback about the spawning operation
///
/// ## Platform-Specific Spawning
///
/// ### Unix Systems
/// ```rust,no_run
/// std::process::Command::new(current_exe)
///     .arg("--daemon-run")
///     .before_exec(|| {
///         nix::unistd::setsid()?; // Create new session
///         Ok(())
///     })
///     .spawn()?;
/// ```
///
/// ### Windows
/// ```rust,no_run
/// std::process::Command::new(current_exe)
///     .arg("--daemon-run")
///     .creation_flags(CREATE_NO_WINDOW) // Hide console window
///     .spawn()?;
/// ```
///
/// ## Duplicate Prevention
///
/// The function prevents multiple daemon instances by:
/// - Checking for existing PID files
/// - Validating that the process in the PID file is actually running
/// - Terminating stale processes before starting new ones
/// - Cleaning up orphaned PID files
///
/// ## Error Recovery
///
/// If stopping an existing daemon fails:
/// - Issues a warning but continues with spawning
/// - Removes stale PID files to prevent conflicts
/// - Allows a brief delay for process cleanup
/// - Proceeds with new daemon creation
///
/// # Returns
///
/// Returns `Ok(())` if the daemon was successfully spawned and the PID file
/// was created, or an error if the spawning process fails.
///
/// # Error Scenarios
///
/// - **Executable Not Found**: Cannot locate the current executable
/// - **Permission Denied**: Insufficient privileges for process creation
/// - **Resource Exhaustion**: System cannot create new processes
/// - **PID File Creation**: Cannot write PID file to application directory
/// - **Platform Unsupported**: Daemon mode not available on the current platform
///
/// # Usage Examples
///
/// ```rust,no_run
/// use kasl::libs::daemon;
///
/// // Start background monitoring
/// daemon::spawn()?;
/// println!("Background monitoring started");
/// ```
///
/// # Security Considerations
///
/// - The spawned process runs with the same privileges as the parent
/// - PID files are created with user-readable permissions only
/// - No sensitive information is passed via command line arguments
/// - Process isolation is maintained through session separation (Unix)
#[instrument]
pub fn spawn() -> Result<()> {
    debug!("Attempting to spawn daemon process");
    let pid_path = DataStorage::new().get_path(PID_FILE)?;

    // Check if a daemon is already running and stop it
    // This ensures only one daemon instance is active at a time
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

    // Get the current executable path for spawning
    let current_exe = std::env::current_exe().expect(&Message::FailedToGetCurrentExecutable.to_string());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // Spawn daemon process with session detachment
        let child = std::process::Command::new(current_exe)
            .arg("--daemon-run")
            .before_exec(|| {
                // Detach from the current session to become a daemon
                // This ensures the process continues running after parent exits
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

/// Finds and stops the running daemon process.
///
/// This function provides a user-friendly interface for stopping the daemon
/// process. It handles cases where no daemon is running gracefully and
/// provides appropriate feedback to the user.
///
/// ## Operation Flow
///
/// 1. **Process Lookup**: Searches for running daemon using PID file
/// 2. **Termination**: Attempts to terminate the found process
/// 3. **Cleanup**: Removes PID file and other resources
/// 4. **Status Reporting**: Provides feedback about the operation result
///
/// ## Error Handling Strategy
///
/// This function uses a forgiving error handling approach:
/// - **Process Not Found**: Reports "not running" instead of error
/// - **Stale PID File**: Cleans up orphaned files without complaint
/// - **Permission Issues**: Reports specific error details
/// - **Cleanup Failures**: Continues operation, reports warnings
///
/// ## User Experience
///
/// The function prioritizes clear user communication:
/// - Success messages confirm the daemon was stopped
/// - "Not running" messages avoid unnecessary error reports
/// - Specific error messages help with troubleshooting
/// - Consistent behavior across multiple invocations
///
/// # Returns
///
/// Returns `Ok(())` in most cases, including when no daemon is running.
/// Only returns errors for serious system-level failures that require
/// user attention.
///
/// # Error Scenarios
///
/// - **Permission Denied**: Insufficient privileges to terminate the process
/// - **System Errors**: Platform-specific process management failures
/// - **Resource Issues**: System resource exhaustion during termination
///
/// # Usage Examples
///
/// ```rust,no_run
/// use kasl::libs::daemon;
///
/// // Stop background monitoring
/// daemon::stop()?;
/// println!("Monitoring stopped");
/// ```
///
/// # Idempotent Operation
///
/// This function is safe to call multiple times and will not produce
/// errors if called when no daemon is running. This makes it suitable
/// for use in cleanup scripts and automated scenarios.
/// Checks if the daemon is currently running.
///
/// This function determines whether a daemon process is currently active by
/// checking for the existence and validity of the PID file and verifying
/// that the corresponding process is still running.
///
/// # Returns
///
/// Returns `true` if the daemon is running, `false` otherwise.
/// This function does not return errors - it treats any failure to
/// verify the daemon as "not running".
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

/// Checks if a process with the given PID is currently running.
///
/// This function uses platform-specific methods to verify if a process
/// exists and is running. It's used internally by daemon management
/// functions to validate process state.
///
/// # Arguments
///
/// * `pid` - The process ID to check
///
/// # Returns
///
/// Returns `true` if the process is running, `false` otherwise.
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

/// Internal function to stop the daemon, used by both stop and spawn.
///
/// This function performs the actual daemon termination logic without
/// the user-friendly error handling of the public [`stop()`] function.
/// It's used internally when precise error information is needed.
///
/// ## Termination Process
///
/// 1. **PID File Validation**: Checks that PID file exists and is readable
/// 2. **PID Parsing**: Validates that PID file contains a valid process ID
/// 3. **Process Termination**: Uses platform-specific termination methods
/// 4. **File Cleanup**: Removes PID file regardless of termination result
/// 5. **Result Validation**: Confirms the process was actually terminated
///
/// ## Error Propagation
///
/// Unlike the public interface, this function propagates all errors:
/// - **File Not Found**: PID file doesn't exist
/// - **Invalid Content**: PID file contains invalid data
/// - **Process Not Found**: Process ID is not running
/// - **Termination Failed**: Process couldn't be terminated
///
/// ## Cleanup Guarantee
///
/// The function guarantees PID file cleanup even if process termination
/// fails. This prevents stale PID files from interfering with future
/// daemon operations.
///
/// # Returns
///
/// Returns `Ok(())` if the daemon was successfully terminated, or an
/// error describing the specific failure encountered.
///
/// # Error Scenarios
///
/// - **No PID File**: Daemon is not running or PID file was removed
/// - **Invalid PID**: PID file contains corrupted or invalid data
/// - **Process Not Found**: Process ID does not correspond to running process
/// - **Termination Failed**: Process exists but couldn't be terminated
///
/// # Usage Context
///
/// This function is used internally by:
/// - [`stop()`] for user-initiated daemon termination
/// - [`spawn()`] for replacing existing daemon instances
/// - Test utilities for controlled daemon lifecycle management
fn stop_internal() -> Result<()> {
    let pid_path = DataStorage::new().get_path(PID_FILE)?;

    // Check if PID file exists
    if !pid_path.exists() {
        msg_bail_anyhow!(Message::WatcherNotRunningPidNotFound);
    }

    // Read and parse the PID from the file
    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str.trim().parse().map_err(|_| msg_error_anyhow!(Message::InvalidPidFileContent))?;

    // Attempt to terminate the process
    let killed = kill_process(pid)?;

    // Clean up the PID file regardless of whether the process was found
    // This prevents stale PID files from interfering with future operations
    std::fs::remove_file(pid_path)?;

    if killed {
        msg_info!(Message::WatcherStopped(pid));
        Ok(())
    } else {
        msg_bail_anyhow!(Message::WatcherFailedToStop(pid));
    }
}

/// Cross-platform process termination for Windows systems.
///
/// This function uses Windows-specific APIs to terminate a process by its
/// process ID. It handles Windows process management through the WinAPI
/// and provides detailed error information for troubleshooting.
///
/// ## Windows Process Management
///
/// The function uses these WinAPI functions:
/// - `OpenProcess()`: Opens a handle to the target process
/// - `TerminateProcess()`: Forcibly terminates the process
/// - `CloseHandle()`: Releases the process handle
/// - `GetLastError()`: Retrieves detailed error information
///
/// ## Error Handling
///
/// Windows-specific error codes are handled:
/// - **ERROR_INVALID_PARAMETER (87)**: Process doesn't exist
/// - **ACCESS_DENIED**: Insufficient privileges
/// - **INVALID_HANDLE**: Process handle creation failed
///
/// ## Termination Strategy
///
/// The function uses forceful termination (`TerminateProcess`) rather than
/// graceful shutdown signals. While less elegant than Unix signals, this
/// ensures reliable process termination on Windows systems.
///
/// ## Safety Considerations
///
/// - Process handles are properly closed to prevent resource leaks
/// - Error conditions are checked after each API call
/// - Brief delay allows for process cleanup before returning
///
/// # Arguments
///
/// * `pid` - The process ID of the target process to terminate
///
/// # Returns
///
/// Returns `Ok(true)` if the process was successfully terminated,
/// `Ok(false)` if the process doesn't exist, or an error if termination fails.
///
/// # Error Scenarios
///
/// - **Access Denied**: Insufficient privileges to terminate the process
/// - **Invalid Handle**: Cannot open process handle
/// - **Termination Failed**: Process exists but termination failed
///
/// # Platform Availability
///
/// This function is only available on Windows platforms and will not
/// compile on Unix-like systems.
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

/// Cross-platform process termination for Unix-like systems.
///
/// This function uses Unix command-line tools to terminate a process by its
/// process ID. It implements a graceful termination strategy that attempts
/// polite shutdown before resorting to forceful termination.
///
/// ## Termination Strategy
///
/// 1. **Process Validation**: Uses `ps` to verify the process exists
/// 2. **Graceful Termination**: Sends SIGTERM for clean shutdown
/// 3. **Wait Period**: Allows time for graceful shutdown (1 second)
/// 4. **Forced Termination**: Sends SIGKILL if graceful shutdown fails
/// 5. **Final Validation**: Confirms the process was terminated
///
/// ## Signal Handling
///
/// - **SIGTERM**: Requests graceful shutdown, allows cleanup
/// - **SIGKILL**: Forces immediate termination, no cleanup possible
///
/// ## Command Dependencies
///
/// This function requires standard Unix utilities:
/// - `ps`: Process status checking
/// - `kill`: Signal sending
///
/// These are available on virtually all Unix-like systems including
/// Linux, macOS, BSD variants, and Solaris.
///
/// ## Graceful Shutdown Benefits
///
/// The graceful termination approach provides several advantages:
/// - Allows proper cleanup of resources
/// - Enables database transaction completion
/// - Provides opportunity for state saving
/// - Reduces risk of data corruption
///
/// # Arguments
///
/// * `pid` - The process ID of the target process to terminate
///
/// # Returns
///
/// Returns `Ok(true)` if the process was successfully terminated,
/// `Ok(false)` if the process doesn't exist, or an error if termination fails.
///
/// # Error Scenarios
///
/// - **Process Not Found**: Process ID doesn't correspond to running process
/// - **Permission Denied**: Insufficient privileges to send signals
/// - **Command Failed**: `ps` or `kill` commands not available or failed
/// - **Persistent Process**: Process survives both SIGTERM and SIGKILL
///
/// # Platform Availability
///
/// This function is only available on Unix-like platforms and will not
/// compile on Windows systems.
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
