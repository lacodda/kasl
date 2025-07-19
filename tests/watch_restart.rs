#[cfg(test)]
mod tests {
    use kasl::libs::data_storage::DataStorage;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    /// Test context for watch restart tests.
    struct WatchRestartTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for WatchRestartTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            WatchRestartTestContext { _temp_dir: temp_dir }
        }

        fn teardown(self) {
            // Clean up any remaining watch processes
            let _ = Command::new("kasl").args(&["watch", "--stop"]).output();
        }
    }

    #[test_context(WatchRestartTestContext)]
    #[test]
    #[ignore] // This test requires the kasl binary to be built
    fn test_watch_automatic_restart(_ctx: &mut WatchRestartTestContext) {
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Start first instance
        let output = Command::new("kasl").arg("watch").output().expect("Failed to start first watch instance");
        assert!(output.status.success());

        // Give it time to start
        thread::sleep(Duration::from_millis(500));

        // Check PID file exists
        assert!(pid_path.exists(), "PID file should exist after starting watch");

        // Read first PID
        let first_pid = std::fs::read_to_string(&pid_path).expect("Failed to read first PID").trim().to_string();

        // Start second instance (should stop the first)
        let output = Command::new("kasl").arg("watch").output().expect("Failed to start second watch instance");
        assert!(output.status.success());

        // Give it time to restart
        thread::sleep(Duration::from_millis(500));

        // Read second PID
        let second_pid = std::fs::read_to_string(&pid_path).expect("Failed to read second PID").trim().to_string();

        // PIDs should be different
        assert_ne!(first_pid, second_pid, "PIDs should be different after restart");

        // Stop the watch
        let output = Command::new("kasl").args(&["watch", "--stop"]).output().expect("Failed to stop watch");
        assert!(output.status.success());

        // PID file should be gone
        assert!(!pid_path.exists(), "PID file should be removed after stopping");
    }
}
