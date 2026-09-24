//! One watcher per user, whoever starts the second one.
//!
//! Two watchers are two Jira pollers with two toast budgets: every toast is
//! shown twice and the hourly cap is doubled. Found in the field with two
//! copies of kasl installed side by side, each started at login by its own
//! autostart entry. These tests start the second watcher every way it can be
//! started and check that the first one stays the only one.

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    struct SingleWatcherContext {
        dir: PathBuf,
        first: Option<Child>,
        _temp_dir: TempDir,
    }

    impl TestContext for SingleWatcherContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            SingleWatcherContext {
                dir: temp_dir.path().to_path_buf(),
                first: None,
                _temp_dir: temp_dir,
            }
        }

        fn teardown(mut self) {
            if let Some(mut first) = self.first.take() {
                let _ = first.kill();
                let _ = first.wait();
            }
            let _ = kasl_cmd(&self.dir).args(["watch", "--stop"]).output();
            thread::sleep(Duration::from_millis(500));
        }
    }

    /// A kasl command bound to the test's own data directory, stdio detached
    /// so a daemon outliving the test cannot hold the harness pipe open.
    fn kasl_cmd(dir: &Path) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kasl"));
        cmd.env("HOME", dir)
            .env("LOCALAPPDATA", dir)
            .env("XDG_DATA_HOME", dir.join(".local").join("share"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }

    /// Where the watcher in `dir` records its PID, found the way kasl finds it.
    ///
    /// The data directory differs per OS, so the path is not spelled out
    /// here: the file is looked for under `dir`.
    fn pid_file(dir: &Path) -> Option<PathBuf> {
        fn walk(dir: &Path) -> Option<PathBuf> {
            for entry in std::fs::read_dir(dir).ok()?.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(found) = walk(&path) {
                        return Some(found);
                    }
                } else if path.file_name().is_some_and(|n| n == "kasl-watch.pid") {
                    return Some(path);
                }
            }
            None
        }
        walk(dir)
    }

    fn recorded_pid(dir: &Path) -> Option<u32> {
        std::fs::read_to_string(pid_file(dir)?).ok()?.trim().parse().ok()
    }

    fn wait_for(mut cond: impl FnMut() -> bool, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if cond() {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
        false
    }

    /// Starts the first watcher and waits until it has registered itself.
    fn start_first(ctx: &mut SingleWatcherContext) -> u32 {
        let first = kasl_cmd(&ctx.dir).arg("--daemon-run").spawn().expect("start the first watcher");
        let pid = first.id();
        ctx.first = Some(first);
        assert!(
            wait_for(|| recorded_pid(&ctx.dir) == Some(pid), Duration::from_secs(30)),
            "the first watcher should record its own PID"
        );
        pid
    }

    fn first_is_alive(ctx: &mut SingleWatcherContext) -> bool {
        ctx.first.as_mut().is_some_and(|c| c.try_wait().unwrap().is_none())
    }

    #[test_context(SingleWatcherContext)]
    #[serial]
    #[test]
    fn a_second_daemon_leaves_the_first_one_running(ctx: &mut SingleWatcherContext) {
        let first = start_first(ctx);

        let mut second = kasl_cmd(&ctx.dir).arg("--daemon-run").spawn().expect("start the second watcher");
        let exited = wait_for(|| second.try_wait().unwrap().is_some(), Duration::from_secs(15));
        if !exited {
            let _ = second.kill();
            let _ = second.wait();
        }
        assert!(exited, "a second watcher must leave while the first one holds the lock");
        assert!(second.wait().unwrap().success(), "leaving is not a failure");

        assert!(first_is_alive(ctx), "the first watcher keeps running");
        assert_eq!(recorded_pid(&ctx.dir), Some(first), "the PID file still names the first watcher");
    }

    #[test_context(SingleWatcherContext)]
    #[serial]
    #[test]
    fn a_foreground_watcher_refuses_beside_a_running_one(ctx: &mut SingleWatcherContext) {
        start_first(ctx);

        let mut foreground = kasl_cmd(&ctx.dir).args(["watch", "--foreground"]).stderr(Stdio::piped()).spawn().unwrap();
        let exited = wait_for(|| foreground.try_wait().unwrap().is_some(), Duration::from_secs(15));
        if !exited {
            let _ = foreground.kill();
        }
        let output = foreground.wait_with_output().unwrap();
        assert!(exited, "a foreground watcher must not start beside a running one");
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("already running"), "the refusal says why: {stderr}");
        assert!(first_is_alive(ctx));
    }

    /// The field case: a watcher the PID file does not name, because another
    /// copy of kasl started it or the PID file was lost.
    #[test_context(SingleWatcherContext)]
    #[serial]
    #[test]
    fn watch_does_not_start_a_second_one_beside_an_untracked_watcher(ctx: &mut SingleWatcherContext) {
        let first = start_first(ctx);
        std::fs::remove_file(pid_file(&ctx.dir).unwrap()).unwrap();

        // Into a file, not a pipe: a watcher `watch` wrongly left running
        // would inherit the pipe and hold it open, and the test would hang
        // instead of failing.
        let out_path = ctx.dir.join("watch.out");
        let out = std::fs::File::create(&out_path).unwrap();
        let status = kasl_cmd(&ctx.dir).arg("watch").stdout(out).status().unwrap();
        let stdout = std::fs::read_to_string(&out_path).unwrap();
        assert!(status.success(), "{stdout}");
        assert!(stdout.contains("already running"), "watch says a watcher was already there: {stdout}");
        assert!(
            !stdout.contains("started in the background"),
            "and does not claim to have started one: {stdout}"
        );

        assert!(first_is_alive(ctx), "the untracked watcher keeps running");
        assert!(recorded_pid(&ctx.dir).is_none_or(|pid| pid == first), "no PID file names the watcher that left");
    }

    /// A PID file left behind by a killed watcher names a number the OS may
    /// have given to someone else; `--stop` must not kill that process.
    #[test_context(SingleWatcherContext)]
    #[serial]
    #[test]
    fn stop_leaves_a_process_that_is_not_kasl_alone(ctx: &mut SingleWatcherContext) {
        start_first(ctx);
        let pid_path = pid_file(&ctx.dir).unwrap();
        let mut first = ctx.first.take().unwrap();
        first.kill().unwrap();
        first.wait().unwrap();

        let mut stranger = if cfg!(windows) {
            Command::new("ping").args(["-n", "60", "127.0.0.1"]).stdout(Stdio::null()).spawn().unwrap()
        } else {
            Command::new("sleep").arg("60").spawn().unwrap()
        };
        std::fs::write(&pid_path, stranger.id().to_string()).unwrap();

        let status = kasl_cmd(&ctx.dir).args(["watch", "--stop"]).status().unwrap();
        let survived = stranger.try_wait().unwrap().is_none();
        let _ = stranger.kill();
        let _ = stranger.wait();

        assert!(status.success(), "a stale PID file is a stopped watcher, not an error");
        assert!(survived, "the process now holding the number is not kasl's to kill");
        assert!(!pid_path.exists(), "the stale PID file is cleared");
    }

    /// The daemon outlives `watch`; it must not keep the caller's stdout.
    #[test_context(SingleWatcherContext)]
    #[serial]
    #[test]
    fn watch_returns_when_its_output_is_read_to_the_end(ctx: &mut SingleWatcherContext) {
        let mut cmd = kasl_cmd(&ctx.dir);
        cmd.arg("watch").stdout(Stdio::piped()).stderr(Stdio::piped());
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(cmd.output());
        });

        let answered = rx.recv_timeout(Duration::from_secs(30));
        if answered.is_err() {
            // Let the reader go before failing, or the harness hangs on it.
            let _ = kasl_cmd(&ctx.dir).args(["watch", "--stop"]).output();
        }
        let output = answered.expect("watch should return while the daemon it started keeps running").unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("Monitor is running"),
            "the daemon writes nothing into watch's output: {stdout}"
        );
    }
}
