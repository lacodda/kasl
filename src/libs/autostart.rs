use crate::libs::messages::Message;
use crate::{msg_debug, msg_error_anyhow, msg_info};
use anyhow::Result;
use std::env;

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const TASK_NAME: &str = "KaslAutostart";
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    /// Convert Windows output from OEM codepage to UTF-8
    pub(crate) fn decode_windows_output(bytes: &[u8]) -> String {
        // Try UTF-8 first
        if let Ok(utf8) = String::from_utf8(bytes.to_vec()) {
            return utf8;
        }

        // Fall back to Windows-1252 or lossy conversion
        encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
    }

    pub fn enable() -> Result<()> {
        let exe_path = env::current_exe()?;
        let exe_path_str = exe_path.to_string_lossy();

        msg_debug!(format!("Creating scheduled task for: {}", exe_path_str));

        // First, try to delete existing task if any
        let _ = Command::new("schtasks")
            .args(&["/Delete", "/TN", TASK_NAME, "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        // Create task using schtasks with simpler approach
        let output = Command::new("schtasks")
            .args(&[
                "/Create",
                "/SC",
                "ONLOGON",
                "/TN",
                TASK_NAME,
                "/TR",
                &format!("\"{}\" watch", exe_path_str),
                "/RL",
                "LIMITED",
                "/F", // Force create
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        if output.status.success() {
            msg_info!(Message::AutostartEnabled);
            Ok(())
        } else {
            let error = decode_windows_output(&output.stderr);
            let stdout = decode_windows_output(&output.stdout);

            msg_debug!(format!("schtasks stderr: {}", error));
            msg_debug!(format!("schtasks stdout: {}", stdout));

            // Check if it's an access denied error
            if error.contains("Access is denied") || error.contains("0x80070005") {
                Err(msg_error_anyhow!(Message::AutostartRequiresAdmin))
            } else {
                Err(msg_error_anyhow!(Message::AutostartEnableFailed(error)))
            }
        }
    }

    pub fn disable() -> Result<()> {
        msg_debug!("Removing scheduled task");

        let output = Command::new("schtasks")
            .args(&["/Delete", "/TN", TASK_NAME, "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        if output.status.success() {
            msg_info!(Message::AutostartDisabled);
            Ok(())
        } else {
            let error = decode_windows_output(&output.stderr);

            // If task doesn't exist, consider it a success
            if error.contains("The system cannot find the file specified") || error.contains("0x80070002") {
                msg_info!(Message::AutostartAlreadyDisabled);
                Ok(())
            } else if error.contains("Access is denied") || error.contains("0x80070005") {
                Err(msg_error_anyhow!(Message::AutostartRequiresAdmin))
            } else {
                Err(msg_error_anyhow!(Message::AutostartDisableFailed(error)))
            }
        }
    }

    pub fn is_enabled() -> Result<bool> {
        let output = Command::new("schtasks")
            .args(&["/Query", "/TN", TASK_NAME, "/FO", "CSV"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        Ok(output.status.success())
    }

    /// Check if running with admin privileges
    pub fn is_admin() -> bool {
        use std::ptr;
        use winapi::um::handleapi::CloseHandle;
        use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
        use winapi::um::securitybaseapi::GetTokenInformation;
        use winapi::um::winnt::{TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};

        unsafe {
            let mut token = ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }

            let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut size = 0;
            let result = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as *mut _,
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut size,
            );

            CloseHandle(token);
            result != 0 && elevation.TokenIsElevated != 0
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod unix {
    use super::*;

    pub fn enable() -> Result<()> {
        Err(msg_error_anyhow!(Message::AutostartNotImplemented))
    }

    pub fn disable() -> Result<()> {
        Err(msg_error_anyhow!(Message::AutostartNotImplemented))
    }

    pub fn is_enabled() -> Result<bool> {
        Ok(false)
    }

    pub fn is_admin() -> bool {
        false
    }
}

/// Enable autostart on system boot
pub fn enable() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        // Check for admin rights on Windows
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

/// Enable user-level autostart via Registry (Windows)
#[cfg(target_os = "windows")]
fn enable_user_autostart() -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let exe_path = env::current_exe()?;
    let exe_path_str = exe_path.to_string_lossy();

    msg_debug!("Trying user-level autostart via Registry");

    // Add to current user's Run key
    let output = Command::new("reg")
        .args(&[
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "Kasl",
            "/t",
            "REG_SZ",
            "/d",
            &format!("\"{}\" watch", exe_path_str),
            "/f",
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

/// Disable autostart on system boot
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

/// Disable user-level autostart via Registry (Windows)
#[cfg(target_os = "windows")]
fn disable_user_autostart() -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = Command::new("reg")
        .args(&["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "Kasl", "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let error = windows::decode_windows_output(&output.stderr);
        if error.contains("The system cannot find") {
            Ok(()) // Already removed
        } else {
            Err(msg_error_anyhow!(Message::AutostartDisableFailed(error)))
        }
    }
}

/// Check if autostart is enabled
pub fn is_enabled() -> Result<bool> {
    #[cfg(target_os = "windows")]
    {
        // Check both Task Scheduler and Registry
        if windows::is_enabled().unwrap_or(false) {
            return Ok(true);
        }

        // Check Registry
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

/// Get autostart status as string
pub fn status() -> Result<String> {
    match is_enabled()? {
        true => Ok("enabled".to_string()),
        false => Ok("disabled".to_string()),
    }
}
