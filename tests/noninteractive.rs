//! Commands must work, or fail cleanly, with no terminal attached.
//!
//! kasl runs from scripts, cron jobs and the watch daemon, none of which can
//! answer a prompt. Every command here is spawned as a real process with stdin
//! detached, because that is the only way to reproduce the condition that made
//! `task add --name X` panic on a prompt nobody could see.

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use tempfile::TempDir;

    /// Builds a kasl command bound to a private data directory, with no stdin.
    fn kasl_cmd(dir: &Path) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kasl"));
        cmd.env("HOME", dir).env("LOCALAPPDATA", dir).stdin(Stdio::null());
        cmd
    }

    #[serial]
    #[test]
    fn task_add_with_a_name_needs_no_terminal() {
        let dir = TempDir::new().unwrap();

        // Regression: the comment and completeness prompts ran even when the
        // name was supplied, and unwrapped the "not a terminal" error, so the
        // whole command panicked instead of creating the task.
        let out = kasl_cmd(dir.path())
            .args(["task", "add", "--name", "Scripted task", "--completeness", "30"])
            .output()
            .unwrap();

        assert!(out.status.success(), "task add failed: {}", String::from_utf8_lossy(&out.stderr));

        let listed = kasl_cmd(dir.path()).args(["task", "list"]).output().unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout);
        assert!(listed.contains("Scripted task"), "task missing from list:\n{listed}");
        assert!(listed.contains("30%"), "completeness not applied:\n{listed}");
    }

    #[serial]
    #[test]
    fn task_add_without_a_name_fails_instead_of_hanging() {
        let dir = TempDir::new().unwrap();

        let out = kasl_cmd(dir.path()).args(["task", "add"]).output().unwrap();

        assert!(!out.status.success(), "expected a refusal without a name");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("--name"), "error should name the missing flag:\n{stderr}");
    }

    #[serial]
    #[test]
    fn destructive_commands_refuse_without_yes() {
        let dir = TempDir::new().unwrap();

        kasl_cmd(dir.path()).args(["task", "add", "--name", "Doomed"]).output().unwrap();

        // Without --yes there is a confirmation to answer, and nobody to answer it.
        let out = kasl_cmd(dir.path()).args(["task", "remove", "1"]).output().unwrap();
        assert!(!out.status.success(), "expected a refusal without --yes");

        // With --yes the removal goes through unattended.
        let out = kasl_cmd(dir.path()).args(["task", "remove", "1", "--yes"]).output().unwrap();
        assert!(out.status.success(), "task remove --yes failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[serial]
    #[test]
    fn read_only_commands_work_unattended() {
        let dir = TempDir::new().unwrap();

        for args in [
            vec!["pauses", "list"],
            vec!["task", "list"],
            vec!["tag", "list"],
            vec!["report"],
            vec!["sum"],
            vec!["completions", "bash"],
        ] {
            let out = kasl_cmd(dir.path()).args(&args).output().unwrap();
            assert!(
                out.status.success(),
                "`kasl {}` failed unattended: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
