#[cfg(test)]
mod tests {
    use kasl::libs::{daemon, data_storage::DataStorage};
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
            let kasl_binary = std::env::current_dir().unwrap().join("target").join("debug").join("kasl.exe");
            let _ = Command::new(&kasl_binary).args(&["watch", "--stop"]).output();
            // Give time for cleanup
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    #[test_context(DaemonTestContext)]
    #[test]
    fn test_daemon_stop_on_windows(_ctx: &mut DaemonTestContext) {
        let kasl_binary = std::env::current_dir().unwrap().join("target").join("debug").join("kasl.exe");
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Clean up any existing daemon first
        let _ = Command::new(&kasl_binary).args(&["watch", "--stop"]).output();
        thread::sleep(Duration::from_millis(500));

        // Start daemon with spawn() instead of blocking output()
        let mut child = Command::new(&kasl_binary)
            .arg("watch")
            .spawn()
            .expect("Failed to start watch process");

        // Give it time to start
        thread::sleep(Duration::from_millis(2000));

        // Check PID file exists
        assert!(pid_path.exists(), "PID file should exist after starting watch");

        // Stop daemon
        let output = Command::new(&kasl_binary).args(&["watch", "--stop"]).output().expect("Failed to stop watch");
        assert!(output.status.success(), "Failed to stop daemon: {:?}", String::from_utf8_lossy(&output.stderr));

        // Give it time to stop
        thread::sleep(Duration::from_millis(1000));

        // PID file should be gone
        assert!(!pid_path.exists(), "PID file should be removed after stopping");
        
        // Clean up the child process if it's still running
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test_context(DaemonTestContext)]
    #[test]
    fn test_no_duplicate_daemons(_ctx: &mut DaemonTestContext) {
        let kasl_binary = std::env::current_dir().unwrap().join("target").join("debug").join("kasl.exe");
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Clean up any existing daemon first
        let _ = Command::new(&kasl_binary).args(&["watch", "--stop"]).output();
        thread::sleep(Duration::from_millis(500));

        // Start first daemon using spawn() instead of output()
        let mut child1 = Command::new(&kasl_binary)
            .arg("watch")
            .spawn()
            .expect("Failed to start first watch");

        // Give it time to start
        thread::sleep(Duration::from_millis(2000));

        // Check that PID file exists
        assert!(pid_path.exists(), "First daemon should create PID file");

        // Read first PID
        let first_pid = std::fs::read_to_string(&pid_path).expect("Failed to read first PID").trim().to_string();

        // Try to start second daemon - this should replace the first one
        let mut child2 = Command::new(&kasl_binary)
            .arg("watch")
            .spawn()
            .expect("Failed to start second watch");

        // Give it time to restart
        thread::sleep(Duration::from_millis(2000));

        // Read second PID if file still exists
        if pid_path.exists() {
            let second_pid = std::fs::read_to_string(&pid_path).expect("Failed to read second PID").trim().to_string();
            // PIDs should be different
            assert_ne!(first_pid, second_pid, "Second daemon should have different PID");
        }

        // Clean up
        let _ = Command::new(&kasl_binary).args(&["watch", "--stop"]).output();
        thread::sleep(Duration::from_millis(500));
        
        // Clean up any remaining child processes
        let _ = child1.kill();
        let _ = child1.wait();
        let _ = child2.kill();
        let _ = child2.wait();
    }

    #[test_context(DaemonTestContext)]
    #[test]
    fn test_daemon_is_running_status(_ctx: &mut DaemonTestContext) {
        // Initially no daemon should be running
        assert!(!daemon::is_running(), "No daemon should be running initially");

        let kasl_binary = std::env::current_dir()
            .expect("Failed to get current directory")
            .join("target")
            .join("debug")
            .join("kasl.exe");

        // Start daemon
        let _child = Command::new(&kasl_binary)
            .args(&["watch"])
            .spawn()
            .expect("Failed to start daemon");

        // Give daemon time to start
        thread::sleep(Duration::from_millis(2000));

        // Check if daemon is now running
        assert!(daemon::is_running(), "Daemon should be running after start");

        // Stop daemon
        let _ = Command::new(&kasl_binary).args(&["watch", "--stop"]).output();
        thread::sleep(Duration::from_millis(1000));

        // Check if daemon is stopped
        assert!(!daemon::is_running(), "Daemon should be stopped after stop command");
    }
}
