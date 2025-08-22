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
            let kasl_binary = std::env::current_dir().unwrap().join("target").join("debug").join("kasl.exe");
            let _ = Command::new(&kasl_binary).args(&["watch", "--stop"]).output();
            // Give time for cleanup
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    #[test_context(WatchRestartTestContext)]
    #[test]
    fn test_watch_automatic_restart(_ctx: &mut WatchRestartTestContext) {
        let kasl_binary = std::env::current_dir().unwrap().join("target").join("debug").join("kasl.exe");
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Clean up any existing daemon first
        let _ = Command::new(&kasl_binary).args(&["watch", "--stop"]).output();
        thread::sleep(Duration::from_millis(500));

        // Start first instance using spawn() to avoid blocking
        let mut child1 = Command::new(&kasl_binary)
            .arg("watch")
            .spawn()
            .expect("Failed to start first watch instance");

        // Give it time to start
        thread::sleep(Duration::from_millis(2000));

        // Check PID file exists
        assert!(pid_path.exists(), "PID file should exist after starting watch");

        // Read first PID
        let first_pid = std::fs::read_to_string(&pid_path).expect("Failed to read first PID").trim().to_string();

        // Start second instance (should stop the first) using spawn()
        let mut child2 = Command::new(&kasl_binary)
            .arg("watch")
            .spawn()
            .expect("Failed to start second watch instance");

        // Give it time to restart
        thread::sleep(Duration::from_millis(2000));

        // Check if PID file still exists and read second PID
        if pid_path.exists() {
            let second_pid = std::fs::read_to_string(&pid_path).expect("Failed to read second PID").trim().to_string();
            // PIDs should be different
            assert_ne!(first_pid, second_pid, "PIDs should be different after restart");
        }

        // Stop the watch
        let output = Command::new(&kasl_binary).args(&["watch", "--stop"]).output().expect("Failed to stop watch");
        assert!(output.status.success());

        // Give time for cleanup
        thread::sleep(Duration::from_millis(1000));

        // PID file should be gone
        assert!(!pid_path.exists(), "PID file should be removed after stopping");
        
        // Clean up any remaining child processes
        let _ = child1.kill();
        let _ = child1.wait();
        let _ = child2.kill();
        let _ = child2.wait();
    }
}
