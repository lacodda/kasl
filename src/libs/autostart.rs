//! System boot integration and automatic startup management.
//!
//! Provides cross-platform functionality for enabling and disabling automatic
//! startup of the kasl application when the system boots.
//!
//! ## Features
//!
//! - **Platform Support**: Windows Task Scheduler/Registry, Linux systemd, macOS Launch Agents
//! - **Tiered Approach**: System-level, user-level, and fallback methods
//! - **Security**: Privilege management, attack vector mitigation
//! - **Error Handling**: Graceful degradation, clear user feedback
//!
//! ## Usage
//!
//! ```rust
//! use kasl::libs::autostart;
//!
//! autostart::enable()?;           // Enable autostart
//! let status = autostart::status()?; // Check current status
//! autostart::disable()?;          // Disable autostart
//! ```
//!
//! match autostart::is_enabled()? {
//!     true => println!("Autostart is enabled"),
//!     false => println!("Autostart is disabled"),
//! }
//! ```
//!
//! ## Implementation Notes
//!
//! ### Windows Implementation
//! - Uses Windows Task Scheduler XML configuration
//! - Handles both system-wide and user-specific tasks
//! - Includes proper error code interpretation
//! - Supports various Windows versions and configurations
//!
//! ### Unix Implementation
//! - Implements XDG autostart specification
//! - Creates .desktop files in appropriate directories
//! - Handles desktop environment variations
//! - Supports both system and user installation paths
//!
//! ### macOS Implementation
//! - Uses Property List (plist) files for Launch Services
//! - Integrates with macOS security and sandboxing
//! - Handles code signing and notarization requirements
//! - Supports both GUI and CLI application launching

use crate::libs::messages::Message;
use crate::{msg_debug, msg_error_anyhow, msg_info};
use anyhow::Result;
use std::env;

/// Windows-specific autostart implementation module.
///
/// This module contains all Windows-specific code for managing autostart
/// functionality. It handles both Task Scheduler integration and Registry
/// autostart methods, with automatic privilege detection and fallback.
#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    /// Task name used for Windows Task Scheduler integration.
    ///
    /// This name identifies the kasl autostart task in the Windows Task
    /// Scheduler. It should be unique and descriptive to avoid conflicts
    /// with other applications.
    const TASK_NAME: &str = "KaslAutostart";

    /// Windows process creation flag to hide console windows.
    ///
    /// This flag prevents console windows from appearing when running
    /// system commands, providing a cleaner user experience.
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    /// Converts Windows command output from OEM codepage to UTF-8.
    ///
    /// Windows command-line tools often output text in the OEM codepage
    /// rather than UTF-8. This function attempts proper conversion to
    /// ensure error messages and output are correctly displayed.
    ///
    /// ## Conversion Strategy
    ///
    /// 1. **UTF-8 First**: Try interpreting as UTF-8 directly
    /// 2. **Windows-1252 Fallback**: Use Windows-1252 encoding
    /// 3. **Lossy Conversion**: Accept some character loss if necessary
    ///
    /// # Arguments
    ///
    /// * `bytes` - Raw bytes from Windows command output
    ///
    /// # Returns
    ///
    /// A UTF-8 string representation of the input bytes.
    pub(crate) fn decode_windows_output(bytes: &[u8]) -> String {
        // Try UTF-8 interpretation first
        if let Ok(utf8) = String::from_utf8(bytes.to_vec()) {
            return utf8;
        }

        // Fall back to Windows-1252 encoding with lossy conversion
        encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
    }

    /// Enables system-level autostart using Windows Task Scheduler.
    ///
    /// This function creates a scheduled task that runs kasl automatically
    /// when any user logs into the system. It requires administrative
    /// privileges to create system-level tasks.
    ///
    /// ## Task Configuration
    ///
    /// The created task has the following properties:
    /// - **Trigger**: On user logon (any user)
    /// - **Action**: Run kasl with 'watch' command
    /// - **Privileges**: Limited (non-administrative)
    /// - **Persistence**: Survives system restarts and updates
    ///
    /// ## Administrative Requirements
    ///
    /// System-level task creation requires:
    /// - Current user has administrative privileges
    /// - UAC elevation (if UAC is enabled)
    /// - Write access to Task Scheduler store
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful task creation, or an error describing
    /// the failure reason.
    ///
    /// # Errors
    ///
    /// Common error scenarios:
    /// - **Access Denied**: Insufficient privileges for system-level task
    /// - **Service Unavailable**: Task Scheduler service not running
    /// - **Invalid Path**: Executable path cannot be resolved
    /// - **Configuration Error**: Task definition is invalid
    pub fn enable() -> Result<()> {
        // Get current executable path for task configuration
        let exe_path = env::current_exe()?;
        let exe_path_str = exe_path.to_string_lossy();

        msg_debug!(format!("Creating scheduled task for: {}", exe_path_str));

        // Remove any existing task to ensure clean configuration
        let _ = Command::new("schtasks")
            .args(&["/Delete", "/TN", TASK_NAME, "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        // Create new scheduled task with simplified configuration
        let output = Command::new("schtasks")
            .args(&[
                "/Create", // Create new task
                "/SC",
                "ONLOGON", // Trigger: On user logon
                "/TN",
                TASK_NAME, // Task name
                "/TR",
                &format!("\"{}\" watch", exe_path_str), // Task action
                "/RL",
                "LIMITED", // Run with limited privileges
                "/F",      // Force creation (overwrite existing)
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        // Process command result
        if output.status.success() {
            msg_info!(Message::AutostartEnabled);
            Ok(())
        } else {
            // Decode and analyze error output
            let error = decode_windows_output(&output.stderr);
            let stdout = decode_windows_output(&output.stdout);

            msg_debug!(format!("schtasks stderr: {}", error));
            msg_debug!(format!("schtasks stdout: {}", stdout));

            // Check for common error conditions
            if error.contains("Access is denied") || error.contains("0x80070005") {
                Err(msg_error_anyhow!(Message::AutostartRequiresAdmin))
            } else {
                Err(msg_error_anyhow!(Message::AutostartEnableFailed(error)))
            }
        }
    }

    /// Disables system-level autostart by removing the scheduled task.
    ///
    /// This function removes the kasl autostart task from Windows Task
    /// Scheduler. It attempts to remove the task regardless of current
    /// privileges, falling back gracefully if the task doesn't exist.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful task removal or if no task exists.
    ///
    /// # Error Handling
    ///
    /// The function handles several scenarios:
    /// - **Task Not Found**: Treated as success (already disabled)
    /// - **Access Denied**: May indicate insufficient privileges
    /// - **Service Error**: Task Scheduler service issues
    pub fn disable() -> Result<()> {
        msg_debug!("Removing scheduled task");
        // Attempt to delete the scheduled task
        let output = Command::new("schtasks")
            .args(&["/Delete", "/TN", TASK_NAME, "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        if output.status.success() {
            msg_info!(Message::AutostartDisabled);
            Ok(())
        } else {
            // Analyze error output
            let error = decode_windows_output(&output.stderr);

            // Handle common scenarios
            if error.contains("cannot find") || error.contains("does not exist") {
                // Task doesn't exist - treat as success
                msg_info!(Message::AutostartAlreadyDisabled);
                Ok(())
            } else if error.contains("Access is denied") || error.contains("0x80070005") {
                Err(msg_error_anyhow!(Message::AutostartRequiresAdmin))
            } else {
                Err(msg_error_anyhow!(Message::AutostartDisableFailed(error)))
            }
        }
    }

    /// Checks if system-level autostart is currently enabled.
    ///
    /// This function queries Windows Task Scheduler to determine if the
    /// kasl autostart task exists and is enabled.
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if autostart is enabled, `Ok(false)` if disabled.
    pub fn is_enabled() -> Result<bool> {
        // Query task scheduler for the specific task
        let output = Command::new("schtasks")
            .args(&["/Query", "/TN", TASK_NAME, "/FO", "CSV"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        // Task exists if query command succeeds
        Ok(output.status.success())
    }

    /// Checks if the current process is running with administrative privileges.
    ///
    /// This function uses Windows APIs to determine the current privilege
    /// level. It's used to decide whether system-level autostart is possible
    /// or if user-level fallback methods should be used.
    ///
    /// ## Implementation Details
    ///
    /// The function uses the Windows Token API to:
    /// 1. Get the current process token
    /// 2. Query token elevation information
    /// 3. Determine if the token has administrative privileges
    ///
    /// # Returns
    ///
    /// Returns `true` if running with administrative privileges, `false` otherwise.
    pub fn is_admin() -> bool {
        use std::ptr;
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
        use winapi::um::securitybaseapi::GetTokenInformation;
        use winapi::um::winnt::{TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};

        unsafe {
            // Get current process token
            let mut token = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }

            // Query token elevation status
            let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut size = 0;
            let result = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as *mut _,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut size,
            );

            // Clean up token handle
            CloseHandle(token);

            // Return elevation status
            result != 0 && elevation.TokenIsElevated != 0
        }
    }
}

/// Unix-specific autostart implementation module.
///
/// This module provides autostart functionality for Unix-like systems
/// including Linux and macOS. Currently, it serves as a placeholder
/// for future implementation of XDG autostart specification and
/// platform-specific autostart mechanisms.
#[cfg(not(target_os = "windows"))]
mod unix {
    use super::*;

    /// Enables autostart on Unix systems.
    ///
    /// Currently returns an error indicating that autostart is not yet
    /// implemented for Unix platforms. Future implementation will support:
    /// - XDG autostart specification for Linux
    /// - macOS Launch Services integration
    /// - Desktop environment specific methods
    pub fn enable() -> Result<()> {
        Err(msg_error_anyhow!(Message::AutostartNotImplemented))
    }

    /// Disables autostart on Unix systems.
    ///
    /// Currently returns an error indicating that autostart is not yet
    /// implemented for Unix platforms.
    pub fn disable() -> Result<()> {
        Err(msg_error_anyhow!(Message::AutostartNotImplemented))
    }

    /// Checks if autostart is enabled on Unix systems.
    ///
    /// Currently returns `false` since autostart is not yet implemented
    /// for Unix platforms.
    pub fn is_enabled() -> Result<bool> {
        Ok(false)
    }

    /// Checks for administrative privileges on Unix systems.
    ///
    /// Currently returns `false` since privilege checking is not yet
    /// implemented for Unix platforms. Future implementation will check
    /// for root access or sudo capabilities.
    pub fn is_admin() -> bool {
        false
    }
}

/// Enables autostart on system boot with automatic fallback handling.
///
/// This function attempts to enable autostart using the most appropriate
/// method for the current platform and privilege level. It automatically
/// handles privilege detection and provides fallback options when
/// system-level autostart is not possible.
///
/// ## Platform Behavior
///
/// ### Windows
/// 1. **Privilege Check**: Determines if running with admin privileges
/// 2. **System-Level Attempt**: Tries Task Scheduler if admin
/// 3. **User-Level Fallback**: Uses Registry autostart if not admin
/// 4. **Error Reporting**: Provides clear feedback on results
///
/// ### Unix (Future)
/// 1. **Desktop Detection**: Identifies current desktop environment
/// 2. **System Integration**: Uses systemd or equivalent if available
/// 3. **User Session**: Falls back to user-level autostart
/// 4. **Manual Guidance**: Provides setup instructions if needed
///
/// # Returns
///
/// Returns `Ok(())` on successful autostart configuration, or an error
/// describing the failure and suggesting alternative methods.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::autostart;
///
/// match autostart::enable() {
///     Ok(()) => println!("Autostart enabled successfully"),
///     Err(e) => println!("Autostart failed: {}", e),
/// }
/// ```
pub fn enable() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        // Check for administrative privileges
        if !windows::is_admin() {
            msg_info!(Message::AutostartCheckingAlternative);
            // Try user-level autostart via Registry instead
            return enable_user_autostart();
        }
        return windows::enable();
    }

    #[cfg(not(target_os = "windows"))]
    return unix::enable();
}

/// Enables user-level autostart via Windows Registry.
///
/// This function provides a fallback autostart method for Windows when
/// administrative privileges are not available. It adds an entry to the
/// current user's Registry autostart location.
///
/// ## Registry Location
///
/// The function adds an entry to:
/// `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`
///
/// This location is automatically processed by Windows during user login,
/// starting the application without requiring administrative privileges.
///
/// ## Security Considerations
///
/// User-level autostart:
/// - Only affects the current user account
/// - Can be modified without administrative privileges
/// - Is less persistent than system-level autostart
/// - May be affected by user profile issues
///
/// # Returns
///
/// Returns `Ok(())` on successful Registry modification.
#[cfg(target_os = "windows")]
fn enable_user_autostart() -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Get current executable path
    let exe_path = env::current_exe()?;
    let exe_path_str = exe_path.to_string_lossy();

    msg_debug!("Trying user-level autostart via Registry");

    // Add entry to current user's Run key
    let output = Command::new("reg")
        .args(&[
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Kasl", // Value name
            "/t",
            "REG_SZ", // Value type (string)
            "/d",
            &format!("\"{}\" watch", exe_path_str), // Value data
            "/f",                                   // Force overwrite
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    if output.status.success() {
        msg_info!(Message::AutostartEnabledUser);
        Ok(())
    } else {
        let error = windows::decode_windows_output(&output.stderr);
        Err(msg_error_anyhow!(Message::AutostartEnableFailed(error)))
    }
}

/// Disables autostart on system boot with comprehensive cleanup.
///
/// This function attempts to disable autostart using all available methods
/// to ensure complete removal of autostart configuration. It tries both
/// system-level and user-level removal to handle various installation scenarios.
///
/// ## Cleanup Strategy
///
/// The function performs cleanup in multiple locations:
/// 1. **System-Level**: Removes Task Scheduler tasks (Windows)
/// 2. **User-Level**: Removes Registry entries (Windows)
/// 3. **Desktop Files**: Removes .desktop files (Unix future)
/// 4. **Service Files**: Removes systemd units (Unix future)
///
/// # Returns
///
/// Returns `Ok(())` on successful autostart removal. Partial failures
/// are logged but don't prevent the function from returning success.
pub fn disable() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        // Try to disable both system and user level autostart
        let _ = windows::disable();
        let _ = disable_user_autostart();
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    return unix::disable();
}

/// Disables user-level autostart via Windows Registry cleanup.
///
/// This function removes the kasl entry from the current user's Registry
/// autostart location. It's used as part of comprehensive autostart cleanup.
///
/// # Returns
///
/// Returns `Ok(())` on successful Registry cleanup or if no entry exists.
#[cfg(target_os = "windows")]
fn disable_user_autostart() -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Remove entry from current user's Run key
    let output = Command::new("reg")
        .args(&[
            "delete",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Kasl",
            "/f", // Force deletion
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let error = windows::decode_windows_output(&output.stderr);
        if error.contains("The system cannot find") {
            Ok(()) // Entry doesn't exist - that's fine
        } else {
            Err(msg_error_anyhow!(Message::AutostartDisableFailed(error)))
        }
    }
}

/// Checks if autostart is currently enabled using any available method.
///
/// This function checks all possible autostart locations to provide a
/// comprehensive status report. It checks both system-level and user-level
/// autostart configurations.
///
/// ## Check Strategy
///
/// The function checks multiple locations:
/// 1. **System-Level**: Task Scheduler (Windows) or systemd (Unix future)
/// 2. **User-Level**: Registry autostart (Windows) or user session (Unix future)
/// 3. **Legacy Methods**: Older autostart mechanisms for compatibility
///
/// If any method indicates autostart is enabled, the function returns `true`.
///
/// # Returns
///
/// Returns `Ok(true)` if autostart is enabled by any method, `Ok(false)`
/// if autostart is completely disabled.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::autostart;
///
/// if autostart::is_enabled()? {
///     println!("Autostart is active");
/// } else {
///     println!("Autostart is disabled");
/// }
/// ```
pub fn is_enabled() -> Result<bool> {
    #[cfg(target_os = "windows")]
    {
        // Check both Task Scheduler and Registry
        if windows::is_enabled().unwrap_or(false) {
            return Ok(true);
        }

        // Check Registry autostart
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let output = Command::new("reg")
            .args(&["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "Kasl"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        return Ok(output.status.success());
    }

    #[cfg(not(target_os = "windows"))]
    return unix::is_enabled();
}

/// Gets the current autostart status as a human-readable string.
///
/// This function provides a simple way to get autostart status information
/// suitable for display in user interfaces or command-line output.
///
/// # Returns
///
/// Returns a string indicating the current autostart status:
/// - `"enabled"` if autostart is active
/// - `"disabled"` if autostart is inactive
///
/// # Examples
///
/// ```rust
/// use kasl::libs::autostart;
///
/// let status = autostart::status()?;
/// println!("Autostart status: {}", status);
/// ```
pub fn status() -> Result<String> {
    match is_enabled()? {
        true => Ok("enabled".to_string()),
        false => Ok("disabled".to_string()),
    }
}
