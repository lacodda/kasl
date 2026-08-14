//! Missing identifiers open a picker on a terminal, and refuse without one.
//!
//! The pickers exist for the human at the keyboard. The risk they introduce is
//! the one `libs/prompt` was written for: a prompt raised where nobody can
//! answer it, which under the daemon or in CI would hang instead of failing.
//! These spawn the real binary with stdin detached, because that is the
//! condition a unit test cannot reproduce.
//!
//! A populated store matters here. With an empty inbox the command refuses for
//! a different reason - nothing to pick - which would pass whether or not the
//! terminal guard works at all.

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeDelta};
    use kasl::db::jira_inbox::{JiraInbox, JiraInboxUpsert};
    use kasl::db::pauses::Pauses;
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

    /// Points the library at `dir` so the seeding below writes where the
    /// spawned binary will read.
    fn point_at(dir: &Path) {
        // SAFETY: every test here is #[serial]
        unsafe {
            std::env::set_var("HOME", dir);
            std::env::set_var("LOCALAPPDATA", dir);
        }
    }

    fn seed_inbox(dir: &Path) {
        point_at(dir);
        JiraInbox::new()
            .unwrap()
            .upsert_batch(&[JiraInboxUpsert {
                issue_key: "KA-1".to_string(),
                issue_id: "1".to_string(),
                summary: "An issue to pick".to_string(),
                status_id: Some("10".to_string()),
                status_name: "Open".to_string(),
                priority: Some("Medium".to_string()),
                priority_rank: 3,
                sort_value: Some(5.0),
                url: "https://jira.example.com/browse/KA-1".to_string(),
                raw_updated: None,
            }])
            .unwrap();
    }

    fn seed_pause(dir: &Path) {
        point_at(dir);
        let start = Local::now().date_naive().and_hms_opt(10, 0, 0).unwrap();
        Pauses::new().unwrap().insert_manual(start, TimeDelta::minutes(15), false, None).unwrap();
    }

    #[serial]
    #[test]
    fn inbox_actions_refuse_to_pick_without_a_terminal() {
        let dir = TempDir::new().unwrap();
        seed_inbox(dir.path());

        // Every action that takes a KEY must refuse the same way; a hang here
        // is the failure this test exists to catch.
        for action in ["pin", "dismiss", "open", "take"] {
            let out = kasl_cmd(dir.path()).args(["inbox", action]).output().unwrap();
            assert!(!out.status.success(), "`inbox {action}` should refuse without a terminal");

            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(stderr.contains("KEY"), "`inbox {action}` should name the argument to pass:\n{stderr}");
        }
    }

    #[serial]
    #[test]
    fn inbox_actions_still_take_an_explicit_key() {
        let dir = TempDir::new().unwrap();
        seed_inbox(dir.path());

        // The picker is a fallback; naming the key keeps working unattended.
        let out = kasl_cmd(dir.path()).args(["inbox", "pin", "KA-1"]).output().unwrap();
        assert!(out.status.success(), "pin by key failed: {}", String::from_utf8_lossy(&out.stderr));

        let listed = kasl_cmd(dir.path()).args(["inbox", "list"]).output().unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout);
        assert!(listed.contains("KA-1"), "issue missing from list:\n{listed}");
    }

    #[serial]
    #[test]
    fn pauses_remove_refuses_to_pick_without_a_terminal() {
        let dir = TempDir::new().unwrap();
        seed_pause(dir.path());

        let out = kasl_cmd(dir.path()).args(["pauses", "remove"]).output().unwrap();
        assert!(!out.status.success(), "`pauses remove` should refuse without a terminal");

        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("ID"), "the refusal should name the argument to pass:\n{stderr}");
    }

    #[serial]
    #[test]
    fn task_show_refuses_to_pick_without_a_terminal() {
        let dir = TempDir::new().unwrap();
        kasl_cmd(dir.path()).args(["task", "add", "--name", "A task"]).output().unwrap();

        // `show` used to require an id; now an empty list of ids means "pick",
        // which must not turn into "show everything" unattended.
        let out = kasl_cmd(dir.path()).args(["task", "show"]).output().unwrap();
        assert!(!out.status.success(), "`task show` should refuse without a terminal");

        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("ID"), "the refusal should name the argument to pass:\n{stderr}");
    }

    #[serial]
    #[test]
    fn tag_actions_refuse_to_pick_without_a_terminal() {
        let dir = TempDir::new().unwrap();
        let created = kasl_cmd(dir.path()).args(["tag", "add", "focus"]).output().unwrap();
        assert!(created.status.success(), "tag add failed: {}", String::from_utf8_lossy(&created.stderr));

        for action in ["edit", "remove"] {
            let out = kasl_cmd(dir.path()).args(["tag", action]).output().unwrap();
            assert!(!out.status.success(), "`tag {action}` should refuse without a terminal");

            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(stderr.contains("TAG"), "`tag {action}` should name the argument to pass:\n{stderr}");
        }
    }

    #[serial]
    #[test]
    fn empty_stores_say_what_would_fill_them() {
        let dir = TempDir::new().unwrap();

        // Nothing to pick is a different refusal, and it should point at the
        // command that populates the list rather than at the missing argument.
        let out = kasl_cmd(dir.path()).args(["inbox", "pin"]).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("inbox sync"), "an empty inbox should point at sync:\n{stderr}");

        let out = kasl_cmd(dir.path()).args(["pauses", "remove"]).output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("pauses list"), "an empty day should point at the list:\n{stderr}");
    }
}
