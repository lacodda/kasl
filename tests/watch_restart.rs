#[cfg(test)]
mod tests {
    use kasl::libs::data_storage::DataStorage;
    use serial_test::serial;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    /// Test context for watch restart tests.
    struct WatchRestartTestContext {
        dir: PathBuf,
        _temp_dir: TempDir,
    }

    impl TestContext for WatchRestartTestContext {
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
            WatchRestartTestContext {
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

    #[test_context(WatchRestartTestContext)]
    #[serial]
    #[test]
    fn test_watch_automatic_restart(ctx: &mut WatchRestartTestContext) {
        let pid_path = DataStorage::new().get_path("kasl-watch.pid").unwrap();

        // Start first instance using spawn() to avoid blocking
        let mut child1 = kasl_cmd(&ctx.dir).arg("watch").spawn().expect("Failed to start first watch instance");

        // Give it time to start
        thread::sleep(Duration::from_millis(2000));

        // Check PID file exists
        assert!(pid_path.exists(), "PID file should exist after starting watch");

        // Read first PID
        let first_pid = std::fs::read_to_string(&pid_path).expect("Failed to read first PID").trim().to_string();

        // Start second instance (should stop the first) using spawn()
        let mut child2 = kasl_cmd(&ctx.dir).arg("watch").spawn().expect("Failed to start second watch instance");

        // Give it time to restart
        thread::sleep(Duration::from_millis(2000));

        // Check if PID file still exists and read second PID
        if pid_path.exists() {
            let second_pid = std::fs::read_to_string(&pid_path).expect("Failed to read second PID").trim().to_string();
            // PIDs should be different
            assert_ne!(first_pid, second_pid, "PIDs should be different after restart");
        }

        // Stop the watch
        let output = kasl_cmd(&ctx.dir).args(["watch", "--stop"]).output().expect("Failed to stop watch");
        assert!(output.status.success());

        // Give time for cleanup
        thread::sleep(Duration::from_millis(1000));

        // PID file should be gone
        assert!(!pid_path.exists(), "PID file should be removed after stopping");

        // Clean up any remaining launcher processes
        let _ = child1.kill();
        let _ = child1.wait();
        let _ = child2.kill();
        let _ = child2.wait();
    }
}
