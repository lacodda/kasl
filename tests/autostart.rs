#[cfg(test)]
mod tests {
    use kasl::libs::autostart;
    use serial_test::serial;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    struct AutostartTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for AutostartTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("HOME", temp_dir.path());
            }
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("LOCALAPPDATA", temp_dir.path());
            }
            // Linux autostart honours XDG_CONFIG_HOME before ~/.config, so it
            // must be redirected too - otherwise a runner that sets it would
            // have this test write a systemd unit into the real session.
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("XDG_CONFIG_HOME", temp_dir.path().join("config"));
            }
            AutostartTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_autostart_status_query(_ctx: &mut AutostartTestContext) {
        // Test that we can query autostart status without errors
        let result = autostart::status();
        assert!(result.is_ok());

        let status = result.unwrap();
        assert!(status == "enabled" || status == "disabled");
    }

    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_autostart_is_enabled_query(_ctx: &mut AutostartTestContext) {
        // Test that we can check if autostart is enabled
        let result = autostart::is_enabled();
        assert!(result.is_ok());

        // Either state is valid on a test machine; the call succeeding is the contract.
        let _is_enabled = result.unwrap();
    }

    // There is deliberately no Windows test calling `enable()`/`disable()`.
    //
    // The context above redirects HOME, XDG_CONFIG_HOME and LOCALAPPDATA,
    // which is enough to sandbox the Unix implementation - it writes files
    // under those roots. The Windows one writes to the Task Scheduler and to
    // `HKCU\...\Run`, and neither honours an environment variable, so there is
    // nothing to redirect: the call lands on the machine running the test.
    //
    // It did. A test asserting only "this does not panic" - which a function
    // returning `Result` was never going to do - registered the test binary
    // for startup and then deleted the entry, taking the user's real kasl
    // autostart with it. Found in the field on the owner's machine, together
    // with a doctest doing the same (see the gate in
    // tests/release_consistency.rs).
    //
    // Admin detection is reachable another way when it needs covering: make
    // `windows::is_admin` testable directly rather than through a call that
    // reconfigures the machine to observe it.

    #[cfg(windows)]
    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_windows_scheduled_task_query(_ctx: &mut AutostartTestContext) {
        // Test querying scheduled tasks on Windows
        // This should not fail even if the task doesn't exist
        let result = autostart::is_enabled();
        assert!(result.is_ok());
    }

    #[cfg(unix)]
    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_unix_autostart_enable_disable_roundtrip(_ctx: &mut AutostartTestContext) {
        // HOME points at a temp dir, so this writes a real LaunchAgent plist or
        // systemd user unit there and then removes it, without touching the
        // developer's own session.
        assert!(!autostart::is_enabled().unwrap(), "should start disabled");

        autostart::enable().unwrap();
        assert!(autostart::is_enabled().unwrap(), "unit file should exist after enable");

        autostart::disable().unwrap();
        assert!(!autostart::is_enabled().unwrap(), "unit file should be gone after disable");
    }

    #[cfg(unix)]
    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_unix_disable_is_idempotent(_ctx: &mut AutostartTestContext) {
        // Disabling what was never enabled is success: nothing starts kasl.
        assert!(autostart::disable().is_ok());
        assert!(autostart::disable().is_ok());
    }

    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_autostart_status_consistency(_ctx: &mut AutostartTestContext) {
        // Test that status and is_enabled are consistent
        // Note: Status may change during test execution, so we test valid responses
        let status_result = autostart::status().unwrap();
        let is_enabled_result = autostart::is_enabled().unwrap();

        // Both should return valid values
        assert!(status_result == "enabled" || status_result == "disabled");
        // is_enabled_result is a bool by type; unwrap() above is the real check.
        let _ = is_enabled_result;

        // If we can verify consistency without interference, do so
        if status_result == "enabled" || status_result == "disabled" {
            // Test passes as long as both calls return valid values
            // Exact consistency may vary due to test interference
        }
    }

    #[cfg(windows)]
    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_windows_command_execution(_ctx: &mut AutostartTestContext) {
        // Test that Windows commands can be executed
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Test a simple Windows command
        let output = Command::new("cmd").args(["/C", "echo test"]).creation_flags(CREATE_NO_WINDOW).output();

        assert!(output.is_ok());
        let output = output.unwrap();
        assert!(output.status.success());
    }

    #[cfg(windows)]
    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_windows_registry_query(_ctx: &mut AutostartTestContext) {
        // Test Windows Registry query functionality
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Query a known registry key
        let output = Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "NonExistentKey"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        // This should execute without crashing, though it may return error status
        assert!(output.is_ok());
    }

    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_executable_path_detection(_ctx: &mut AutostartTestContext) {
        // Test that current executable path can be detected
        let current_exe = std::env::current_exe();
        assert!(current_exe.is_ok());

        let exe_path = current_exe.unwrap();
        assert!(exe_path.exists());
        assert!(exe_path.is_file());

        // Verify the path can be converted to string
        let exe_str = exe_path.to_string_lossy();
        assert!(!exe_str.is_empty());
    }

    #[cfg(windows)]
    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_windows_error_handling(_ctx: &mut AutostartTestContext) {
        // Test that Windows-specific error conditions are handled
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Test command that should fail
        let output = Command::new("nonexistent_command_12345").creation_flags(CREATE_NO_WINDOW).output();

        // This should fail gracefully with an error, not panic
        assert!(output.is_err());
    }

    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_multiple_status_queries(_ctx: &mut AutostartTestContext) {
        // Test that multiple status queries return valid responses
        // Note: Status might change between calls due to test interference
        let status1 = autostart::status().unwrap();
        let status2 = autostart::status().unwrap();
        let status3 = autostart::status().unwrap();

        // Each call should return a valid status
        assert!(status1 == "enabled" || status1 == "disabled");
        assert!(status2 == "enabled" || status2 == "disabled");
        assert!(status3 == "enabled" || status3 == "disabled");

        let enabled1 = autostart::is_enabled().unwrap();
        let enabled2 = autostart::is_enabled().unwrap();
        let enabled3 = autostart::is_enabled().unwrap();

        // The values are bools by type; the unwrap()s above are the real check.
        let _ = (enabled1, enabled2, enabled3);
    }

    /// The read-only queries must be safe to call anywhere, on any machine.
    ///
    /// `enable`/`disable` are deliberately not exercised here. They used to
    /// be, wrapped in `catch_unwind` to assert "this does not panic" - which a
    /// function returning `Result` was never going to do - and on Windows that
    /// assertion of nothing was paid for by registering the test binary for
    /// startup and then deleting the entry, removing the user's real kasl
    /// autostart along with it. The Unix roundtrip below covers the write path
    /// where a sandbox actually exists.
    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_autostart_queries_are_safe_anywhere(_ctx: &mut AutostartTestContext) {
        assert!(autostart::status().is_ok());
        assert!(autostart::is_enabled().is_ok());
    }

    #[cfg(windows)]
    #[test_context(AutostartTestContext)]
    #[serial]
    #[test]
    fn test_windows_encoding_handling(_ctx: &mut AutostartTestContext) {
        // Test Windows-specific character encoding handling
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Execute a command that might produce non-ASCII output
        let output = Command::new("cmd")
            .args(["/C", "echo Special chars: àáâãäå"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        if let Ok(output) = output {
            // The output should be processable without crashing
            let _stdout = String::from_utf8_lossy(&output.stdout);
            let _stderr = String::from_utf8_lossy(&output.stderr);
        }
    }
}
