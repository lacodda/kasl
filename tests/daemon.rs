#[cfg(test)]
mod tests {
    use kasl::libs::{daemon, data_storage::DataStorage};
    use serial_test::serial;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    /// Test context for daemon tests.
    struct DaemonTestContext {
        dir: PathBuf,
        _temp_dir: TempDir,
    }

    impl TestContext for DaemonTestContext {
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
            DaemonTestContext {
                dir: temp_dir.path().to_path_buf(),
                _temp_dir: temp_dir,
            }
        }

        fn teardown(self) {
            // Stop any watcher this test may have left behind
            let _ = kasl_cmd(&self.dir).args(["watch", "--stop"]).output();
            thread::sleep(Duration::from_millis(500));
        }
    }

    /// Builds a kasl command bound to the test's own data directory.
    ///
    /// The data-dir env is passed explicitly because parallel tests mutate the
    /// process-global env, and stdio is fully detached: a spawned daemon that
    /// outlives the test must not hold the harness stdout pipe open, or
    /// `cargo test` blocks forever waiting for the pipe to close.
    fn kasl_cmd(dir: &Path) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kasl"));
        cmd.env("HOME", dir)
            .env("LOCALAPPDATA", dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }

    #[test_context(DaemonTestContext)]
    #[serial]
    #[test]
    fn test_daemon_stop(ctx: &mut DaemonTestContext) {
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Start daemon with spawn() instead of blocking output()
        let mut child = kasl_cmd(&ctx.dir).arg("watch").spawn().expect("Failed to start watch process");

        // Give it time to start
        thread::sleep(Duration::from_millis(2000));

        // Check PID file exists
        assert!(pid_path.exists(), "PID file should exist after starting watch");

        // Stop daemon
        let output = kasl_cmd(&ctx.dir).args(["watch", "--stop"]).output().expect("Failed to stop watch");
        assert!(output.status.success(), "watch --stop should succeed");

        // Give it time to stop
        thread::sleep(Duration::from_millis(1000));

        // PID file should be gone
        assert!(!pid_path.exists(), "PID file should be removed after stopping");

        // Clean up the launcher process if it's still running
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test_context(DaemonTestContext)]
    #[serial]
    #[test]
    fn test_no_duplicate_daemons(ctx: &mut DaemonTestContext) {
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Start first daemon using spawn() instead of output()
        let mut child1 = kasl_cmd(&ctx.dir).arg("watch").spawn().expect("Failed to start first watch");

        // Give it time to start
        thread::sleep(Duration::from_millis(2000));

        // Check that PID file exists
        assert!(pid_path.exists(), "First daemon should create PID file");

        // Read first PID
        let first_pid = std::fs::read_to_string(&pid_path).expect("Failed to read first PID").trim().to_string();

        // Try to start second daemon - this should replace the first one
        let mut child2 = kasl_cmd(&ctx.dir).arg("watch").spawn().expect("Failed to start second watch");

        // Give it time to restart
        thread::sleep(Duration::from_millis(2000));

        // Read second PID if file still exists
        if pid_path.exists() {
            let second_pid = std::fs::read_to_string(&pid_path).expect("Failed to read second PID").trim().to_string();
            // PIDs should be different
            assert_ne!(first_pid, second_pid, "Second daemon should have different PID");
        }

        // Clean up
        let _ = kasl_cmd(&ctx.dir).args(["watch", "--stop"]).output();
        thread::sleep(Duration::from_millis(500));

        // Clean up any remaining launcher processes
        let _ = child1.kill();
        let _ = child1.wait();
        let _ = child2.kill();
        let _ = child2.wait();
    }

    #[test_context(DaemonTestContext)]
    #[serial]
    #[test]
    fn test_daemon_is_running_status(ctx: &mut DaemonTestContext) {
        // Initially no daemon should be running
        assert!(!daemon::is_running(), "No daemon should be running initially");

        // Start daemon
        let mut child = kasl_cmd(&ctx.dir).args(["watch"]).spawn().expect("Failed to start daemon");

        // Give daemon time to start
        thread::sleep(Duration::from_millis(2000));

        // Check if daemon is now running
        assert!(daemon::is_running(), "Daemon should be running after start");

        // Stop daemon
        let _ = kasl_cmd(&ctx.dir).args(["watch", "--stop"]).output();
        thread::sleep(Duration::from_millis(1000));

        // Check if daemon is stopped
        assert!(!daemon::is_running(), "Daemon should be stopped after stop command");

        // Reap the launcher so no test process outlives the suite
        let _ = child.kill();
        let _ = child.wait();
    }
}
