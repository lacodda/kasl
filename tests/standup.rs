//! `kasl standup` run as the binary against a seeded sandbox: the markdown a
//! day and a week come to, and `--append` placing it into a note and, run
//! again, replacing it there rather than adding a second copy.

use chrono::{Datelike, Duration, Local, NaiveDate};
use kasl::db::inbox::{Inbox, InboxUpsert};
use serial_test::serial;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn sandbox() -> TempDir {
    let dir = TempDir::new().unwrap();
    // SAFETY: tests touching the env are #[serial]
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("LOCALAPPDATA", dir.path());
    }
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kasl"));
    command.args(args).env("HOME", dir).env("LOCALAPPDATA", dir).stdin(Stdio::null());
    for name in ["NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE", "KASL_DEBUG", "RUST_LOG"] {
        command.env_remove(name);
    }
    command.output().expect("kasl runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn yesterday() -> NaiveDate {
    Local::now().date_naive() - Duration::days(1)
}

/// Yesterday, closed: 09:00 to 17:00 with a half-hour lunch, so 7h 30m of
/// work. A task done on an issue the inbox knows, and one still open.
fn seed(dir: &Path) {
    for args in [
        &[
            "task",
            "add",
            "--name",
            "KA-7 Fix the login timeout",
            "--comment",
            "stale cookie",
            "--completeness",
            "100",
        ][..],
        &["task", "add", "--name", "Draft the migration", "--completeness", "40"][..],
    ] {
        let out = run(dir, args);
        assert!(out.status.success(), "seeding {args:?}: {}", stderr(&out));
    }

    let day = yesterday().format("%Y-%m-%d").to_string();
    let db = kasl::db::db::Db::new().unwrap();
    db.conn
        .execute(
            "INSERT INTO workdays (date, start, end) VALUES (?1, ?2, ?3)",
            rusqlite::params![day, format!("{day} 09:00:00"), format!("{day} 17:00:00")],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO pauses (start, end, duration, protected) VALUES (?1, ?2, 1800, 1)",
            rusqlite::params![format!("{day} 12:00:00"), format!("{day} 12:30:00")],
        )
        .unwrap();
    db.conn
        .execute("UPDATE tasks SET timestamp = ?1", rusqlite::params![format!("{day} 10:00:00")])
        .unwrap();
    db.conn.execute("UPDATE tasks SET issue_key = 'KA-7' WHERE name LIKE 'KA-7%'", []).unwrap();

    Inbox::new()
        .unwrap()
        .upsert_batch(&[InboxUpsert {
            issue_key: "KA-7".to_string(),
            issue_id: "7".to_string(),
            summary: "Fix the login timeout".to_string(),
            status_id: Some("10".to_string()),
            status_name: "Open".to_string(),
            priority: Some("High".to_string()),
            priority_rank: 2,
            sort_value: None,
            url: "https://jira.example.com/browse/KA-7".to_string(),
            raw_updated: None,
            issue_type: Some("Bug".to_string()),
            project: Some("KA".to_string()),
            source: "jira".to_string(),
        }])
        .unwrap();
}

#[test]
#[serial]
fn a_day_is_its_hours_and_its_tasks() {
    let dir = sandbox();
    seed(dir.path());

    let out = run(dir.path(), &["standup", "--last"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let expected = format!(
        "## {}\n\
         \n\
         Worked 7h 30m, 09:00-17:00.\n\
         \n\
         **Done**\n\
         \n\
         - [KA-7](https://jira.example.com/browse/KA-7) Fix the login timeout - stale cookie\n\
         \n\
         **In progress**\n\
         \n\
         - Draft the migration (40%)\n",
        yesterday().format("%A, %B %-d, %Y")
    );
    assert_eq!(stdout(&out), expected);

    // The same day named by its date reads the same.
    let by_date = run(dir.path(), &["standup", "--date", &yesterday().format("%Y-%m-%d").to_string()]);
    assert_eq!(stdout(&by_date), expected);
}

#[test]
#[serial]
fn a_week_counts_its_days() {
    let dir = sandbox();
    seed(dir.path());

    let out = run(dir.path(), &["standup", "--week", "--date", &yesterday().format("%Y-%m-%d").to_string()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.starts_with(&format!("## Week {}, ", yesterday().iso_week().week())), "{text}");
    assert!(text.contains("Worked 7h 30m over 1 day."), "{text}");
    assert!(text.contains(&format!("- {}: 7h 30m", yesterday().format("%A, %B %-d"))), "{text}");
    assert!(text.contains("- Draft the migration (40%)"), "{text}");
}

#[test]
#[serial]
fn append_places_the_summary_once_however_often_it_runs() {
    let dir = sandbox();
    seed(dir.path());
    let notes = dir.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    let note = notes.join(format!("{}.md", yesterday().format("%Y-%m-%d")));
    std::fs::write(&note, "# Daily note\n\nMet the team.\n").unwrap();
    let template = notes.join("{date}.md").display().to_string();

    let first = run(dir.path(), &["standup", "--last", "--append", &template]);
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(stdout(&first).contains("Added the standup"), "{}", stdout(&first));
    let once = std::fs::read_to_string(&note).unwrap();
    assert!(once.starts_with("# Daily note\n\nMet the team.\n\n<!-- kasl standup "), "{once}");
    assert!(once.contains("Worked 7h 30m"), "{once}");

    let second = run(dir.path(), &["standup", "--last", "--append", &template]);
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(stdout(&second).contains("Replaced the standup"), "{}", stdout(&second));
    let twice = std::fs::read_to_string(&note).unwrap();
    assert_eq!(twice, once, "a second run must leave the note as the first one did");
    assert_eq!(twice.matches("<!-- kasl standup ").count(), 1, "{twice}");
}

#[test]
#[serial]
fn append_creates_the_note_but_not_its_folder() {
    let dir = sandbox();
    seed(dir.path());

    let note = dir.path().join("fresh.md");
    let out = run(dir.path(), &["standup", "--last", "--append", &note.display().to_string()]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(std::fs::read_to_string(&note).unwrap().starts_with("<!-- kasl standup "));

    let nowhere = dir.path().join("no-such-folder").join("note.md");
    let out = run(dir.path(), &["standup", "--last", "--append", &nowhere.display().to_string()]);
    assert!(!out.status.success(), "a missing folder must fail");
    assert!(stderr(&out).contains("There is no folder"), "{}", stderr(&out));
    assert!(!nowhere.parent().unwrap().exists(), "the folder must not be created");
}

#[test]
#[serial]
fn nothing_recorded_writes_nothing() {
    let dir = sandbox();
    seed(dir.path());

    let note = dir.path().join("empty.md");
    let out = run(dir.path(), &["standup", "--date", "2001-01-01", "--append", &note.display().to_string()]);
    assert!(out.status.success(), "a day with nothing on it is not a failure: {}", stderr(&out));
    assert!(stdout(&out).contains("Nothing was recorded for Monday, January 1, 2001"), "{}", stdout(&out));
    assert!(!note.exists(), "nothing to say, so no note");
}
