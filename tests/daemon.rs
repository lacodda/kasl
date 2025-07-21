#[cfg(test)]
mod tests {
    use kasl::libs::data_storage::DataStorage;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    /// Test context for daemon tests.
    struct DaemonTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for DaemonTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            DaemonTestContext { _temp_dir: temp_dir }
        }

        fn teardown(self) {
            // Clean up any remaining watch processes
            let _ = Command::new("kasl").args(&["watch", "--stop"]).output();
        }
    }

    #[test_context(DaemonTestContext)]
    #[test]
    #[ignore] // This test requires the kasl binary to be built
    fn test_daemon_stop_on_windows(_ctx: &mut DaemonTestContext) {
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Start daemon
        let output = Command::new("kasl").arg("watch").output().expect("Failed to start watch");
        assert!(output.status.success(), "Failed to start daemon: {:?}", String::from_utf8_lossy(&output.stderr));

        // Give it time to start
        thread::sleep(Duration::from_millis(1000));

        // Check PID file exists
        assert!(pid_path.exists(), "PID file should exist after starting watch");

        // Stop daemon
        let output = Command::new("kasl").args(&["watch", "--stop"]).output().expect("Failed to stop watch");
        assert!(output.status.success(), "Failed to stop daemon: {:?}", String::from_utf8_lossy(&output.stderr));

        // Give it time to stop
        thread::sleep(Duration::from_millis(500));

        // PID file should be gone
        assert!(!pid_path.exists(), "PID file should be removed after stopping");
    }

    #[test_context(DaemonTestContext)]
    #[test]
    #[ignore] // This test requires the kasl binary to be built
    fn test_no_duplicate_daemons(_ctx: &mut DaemonTestContext) {
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Start first daemon
        let output = Command::new("kasl").arg("watch").output().expect("Failed to start first watch");
        assert!(output.status.success());

        // Give it time to start
        thread::sleep(Duration::from_millis(1000));

        // Read first PID
        let first_pid = std::fs::read_to_string(&pid_path).expect("Failed to read first PID").trim().to_string();

        // Try to start second daemon
        let output = Command::new("kasl").arg("watch").output().expect("Failed to start second watch");
        assert!(output.status.success(), "Second watch should succeed by stopping the first");

        // Give it time to restart
        thread::sleep(Duration::from_millis(1000));

        // Read second PID
        let second_pid = std::fs::read_to_string(&pid_path).expect("Failed to read second PID").trim().to_string();

        // PIDs should be different
        assert_ne!(first_pid, second_pid, "Second daemon should have different PID");

        // Clean up
        let _ = Command::new("kasl").args(&["watch", "--stop"]).output();
    }
}
