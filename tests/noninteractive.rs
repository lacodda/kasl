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
    fn task_add_from_a_template_needs_no_terminal() {
        let dir = TempDir::new().unwrap();

        // Regression: `--template` reached dialoguer's prompts unconditionally
        // and died with a bare "IO error: not a terminal" - on the very path
        // `--from-template` tells scripts to use instead of itself.
        kasl_cmd(dir.path())
            .args([
                "template",
                "add",
                "--name",
                "review",
                "--task-name",
                "Code review",
                "--comment",
                "daily",
                "--completeness",
                "80",
            ])
            .output()
            .unwrap();

        let out = kasl_cmd(dir.path()).args(["task", "add", "--template", "review"]).output().unwrap();
        assert!(
            out.status.success(),
            "task add --template failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );

        let listed = kasl_cmd(dir.path()).args(["task", "list"]).output().unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout);
        assert!(
            listed.contains("Code review"),
            "template task missing from list:
{listed}"
        );
        assert!(
            listed.contains("80%"),
            "template completeness not applied:
{listed}"
        );
    }

    #[serial]
    #[test]
    fn flags_win_over_the_template_they_are_passed_with() {
        let dir = TempDir::new().unwrap();

        // The docs promised `--template X --name Y` would apply Y. The flags
        // were silently dropped instead, so the task came out named after the
        // template.
        kasl_cmd(dir.path())
            .args([
                "template",
                "add",
                "--name",
                "review",
                "--task-name",
                "Code review",
                "--comment",
                "daily",
                "--completeness",
                "80",
            ])
            .output()
            .unwrap();

        let out = kasl_cmd(dir.path())
            .args(["task", "add", "--template", "review", "--name", "Review PR 318", "--completeness", "40"])
            .output()
            .unwrap();
        assert!(out.status.success(), "task add failed: {}", String::from_utf8_lossy(&out.stderr));

        let listed = kasl_cmd(dir.path()).args(["task", "list"]).output().unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout);
        assert!(
            listed.contains("Review PR 318"),
            "--name did not win over the template:
{listed}"
        );
        assert!(
            !listed.contains("Code review"),
            "the template name was used despite --name:
{listed}"
        );
        assert!(
            listed.contains("40%"),
            "--completeness did not win over the template:
{listed}"
        );
        // The comment was not overridden, so it still comes from the template.
        assert!(
            listed.contains("daily"),
            "template comment was lost:
{listed}"
        );
    }

    #[serial]
    #[test]
    fn template_add_without_a_name_fails_instead_of_hanging() {
        let dir = TempDir::new().unwrap();

        // `template add` had no flags for its fields at all, so creating one
        // from a script was impossible: it prompted, unwrapped, and died with
        // "IO error: not a terminal" instead of naming the flag to pass.
        let out = kasl_cmd(dir.path()).args(["template", "add"]).output().unwrap();

        assert!(!out.status.success(), "expected a refusal without a name");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("--name"),
            "error should name the missing flag:
{stderr}"
        );
        assert!(
            !stderr.contains("not a terminal"),
            "the raw dialoguer error leaked through:
{stderr}"
        );
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
