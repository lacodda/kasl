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

    /// Polls a condition until it holds or the deadline passes.
    ///
    /// CI runners vary wildly in speed, so fixed sleeps flake: a cold macOS
    /// runner can take far longer than 2 s to start the daemon. Polling with
    /// a generous deadline is fast on quick machines and patient on slow ones.
    fn wait_for(cond: impl Fn() -> bool, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if cond() {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
        false
    }

    #[test_context(DaemonTestContext)]
    #[serial]
    #[test]
    fn test_daemon_stop(ctx: &mut DaemonTestContext) {
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Start daemon with spawn() instead of blocking output()
        let mut child = kasl_cmd(&ctx.dir).arg("watch").spawn().expect("Failed to start watch process");

        // Wait for the daemon to come up and write its PID file
        assert!(
            wait_for(|| pid_path.exists(), Duration::from_secs(30)),
            "PID file should exist after starting watch"
        );

        // Stop daemon (capture output for diagnostics on failure)
        let output = kasl_cmd(&ctx.dir)
            .args(["watch", "--stop"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("Failed to stop watch");
        assert!(
            output.status.success(),
            "watch --stop should succeed
stdout: {}
stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        // PID file should be gone once the stop settles
        assert!(
            wait_for(|| !pid_path.exists(), Duration::from_secs(10)),
            "PID file should be removed after stopping"
        );

        // Clean up the launcher process if it's still running
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test_context(DaemonTestContext)]
    #[serial]
    #[test]
    fn test_no_duplicate_daemons(ctx: &mut DaemonTestContext) {
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();
        let pid_path_clone = pid_path.clone();

        // Start first daemon using spawn() instead of output()
        let mut child1 = kasl_cmd(&ctx.dir).arg("watch").spawn().expect("Failed to start first watch");

        // Wait for the first daemon to come up and write its PID file
        assert!(wait_for(|| pid_path.exists(), Duration::from_secs(30)), "First daemon should create PID file");

        // Read first PID
        let first_pid = std::fs::read_to_string(&pid_path).expect("Failed to read first PID").trim().to_string();

        // Try to start second daemon - this should replace the first one
        let mut child2 = kasl_cmd(&ctx.dir).arg("watch").spawn().expect("Failed to start second watch");

        // Wait for the restart to settle: either the PID changes or the file
        // is (transiently) gone while the first daemon shuts down
        let first = first_pid.clone();
        wait_for(
            move || match std::fs::read_to_string(&pid_path_clone) {
                Ok(content) => content.trim() != first,
                Err(_) => true,
            },
            Duration::from_secs(30),
        );

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

        // Wait for the daemon to come up
        assert!(wait_for(daemon::is_running, Duration::from_secs(30)), "Daemon should be running after start");

        // Stop daemon
        let _ = kasl_cmd(&ctx.dir).args(["watch", "--stop"]).output();

        // Wait for the daemon to go down
        assert!(
            wait_for(|| !daemon::is_running(), Duration::from_secs(10)),
            "Daemon should be stopped after stop command"
        );

        // Reap the launcher so no test process outlives the suite
        let _ = child.kill();
        let _ = child.wait();
    }
}
