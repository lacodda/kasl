//! `--anon`, run as the binary against a sandbox whose every written word is marked.
//!
//! The promise is that a transcript taken with `--anon` carries none of the
//! person's words. So the sandbox is seeded with one word - `zebra` - in
//! every place a person or a tracker writes, and every command that takes
//! the flag is run in every style: the word must be in the output without
//! the flag (or the check proves nothing) and nowhere in it with the flag.

use chrono::{Duration, Local};
use kasl::db::inbox::{Inbox, InboxUpsert};
use kasl::db::workdays::Workdays;
use serde_json::Value;
use serial_test::serial;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

/// The word that must not survive.
const MARK: &str = "zebra";

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

/// Everything a run said, on either stream.
fn said(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn ok(dir: &Path, args: &[&str]) -> Output {
    let out = run(dir, args);
    assert!(out.status.success(), "kasl {args:?} failed:\n{}", said(&out));
    out
}

fn document(dir: &Path, args: &[&str]) -> Value {
    let out = ok(dir, args);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("kasl {args:?}: not a JSON document ({error}):\n{text}"))
}

/// A day whose every written word carries the mark: a task taken from an
/// issue, with a comment and a tag; a task typed by hand; a template; the
/// issue itself, with its project and its address.
fn seed(dir: &Path) {
    for args in [
        &[
            "task",
            "add",
            "--name",
            "Zebra crossing repaint",
            "--comment",
            "ask the zebra keeper",
            "--tags",
            "zebratag",
            "--completeness",
            "40",
        ][..],
        &[
            "template",
            "add",
            "--name",
            "zebra-walk",
            "--task-name",
            "Walk the zebra",
            "--comment",
            "zebra, daily",
        ][..],
    ] {
        ok(dir, args);
    }

    let today = Local::now().date_naive();
    Workdays::new().unwrap().insert_start(today).unwrap();
    let start = (Local::now().naive_local() - Duration::hours(3)).max(today.and_hms_opt(0, 0, 1).unwrap());
    let db = kasl::db::db::Db::new().unwrap();
    db.conn
        .execute(
            "UPDATE workdays SET start = ?1",
            rusqlite::params![start.format("%Y-%m-%d %H:%M:%S").to_string()],
        )
        .unwrap();

    Inbox::new()
        .unwrap()
        .upsert_batch(&[InboxUpsert {
            issue_key: "ZEBRA-77".to_string(),
            issue_id: "1077".to_string(),
            summary: "Zebra enclosure gate sticks".to_string(),
            status_id: Some("10".to_string()),
            status_name: "Open".to_string(),
            priority: Some("High".to_string()),
            priority_rank: 2,
            sort_value: Some(8.0),
            url: "https://zebra.example.com/browse/ZEBRA-77".to_string(),
            raw_updated: None,
            issue_type: Some("Bug".to_string()),
            project: Some("ZEBRA".to_string()),
            source: "jira".to_string(),
        }])
        .unwrap();
    // Pinned by a rule the person named, and taken into a task that opens with its key.
    db.conn.execute("UPDATE inbox SET pinned = 1, ruled_by = 'zebra-first'", []).unwrap();
    db.conn
        .execute(
            "INSERT INTO tasks (task_id, timestamp, name, comment, completeness, excluded_from_search, issue_key)
             VALUES (0, datetime(CURRENT_TIMESTAMP, 'localtime'), 'ZEBRA-77 Zebra enclosure gate sticks', '', 20, 0, 'ZEBRA-77')",
            [],
        )
        .unwrap();
    db.conn.execute("UPDATE tasks SET task_id = id WHERE task_id = 0", []).unwrap();
}

/// Every command that takes the flag, in the styles it has.
fn showing_commands() -> Vec<Vec<&'static str>> {
    let with_json: &[&[&str]] = &[
        &["report"],
        &["task", "list"],
        &["task", "list", "--all"],
        &["task", "list", "--tag", "zebratag"],
        &["task", "show", "1", "2"],
        &["inbox"],
        &["inbox", "list", "--all"],
        &["inbox", "show", "ZEBRA-77"],
        &["tag", "list"],
        &["template", "list"],
    ];
    let text_only: &[&[&str]] = &[&["standup"], &["standup", "--week"]];

    let mut commands = Vec::new();
    for base in with_json {
        for style in [&[][..], &["--plain"], &["--json"]] {
            commands.push(base.iter().chain(style).copied().collect());
        }
    }
    for base in text_only {
        for style in [&[][..], &["--plain"]] {
            commands.push(base.iter().chain(style).copied().collect());
        }
    }
    commands
}

#[test]
#[serial]
fn no_written_word_survives_in_any_command_or_style() {
    let dir = sandbox();
    seed(dir.path());

    for command in showing_commands() {
        // `task list --tag zebratag` echoes nothing, but its own arguments
        // are the person's to type; what is checked is what kasl prints.
        let plain = said(&ok(dir.path(), &command)).to_lowercase();
        assert!(
            plain.contains(MARK),
            "kasl {command:?} does not show the mark at all, so it proves nothing:\n{plain}"
        );

        let mut anon = command.clone();
        anon.push("--anon");
        let masked = said(&ok(dir.path(), &anon)).to_lowercase();
        assert!(!masked.contains(MARK), "kasl {anon:?} let a written word through:\n{masked}");
    }
}

#[test]
#[serial]
fn a_stand_in_is_the_same_in_every_command_and_on_every_run() {
    let dir = sandbox();
    seed(dir.path());

    let names = |args: &[&str]| -> Vec<String> {
        document(dir.path(), args)["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|task| task["name"].as_str().unwrap().to_string())
            .collect()
    };
    let listed = names(&["task", "list", "--anon", "--json"]);
    assert_eq!(listed, names(&["task", "list", "--anon", "--json"]), "the same on a second run");
    assert_eq!(listed, names(&["report", "--anon", "--json"]), "and in the report");
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|name| name.contains("Task ")), "{listed:?}");

    // The task taken from the issue still opens with the issue's key - the stand-in one.
    let issue = document(dir.path(), &["inbox", "show", "ZEBRA-77", "--anon", "--json"]);
    let key = issue["issue"]["key"].as_str().unwrap();
    assert!(listed.iter().any(|name| name.starts_with(key)), "{key} opens one of {listed:?}");
    assert_eq!(issue["issue"]["project"].as_str().unwrap(), key.split('-').next().unwrap());
    assert!(issue["issue"]["url"].as_str().unwrap().starts_with("https://tracker.example/"));

    // What kasl counted is as it was.
    let with = document(dir.path(), &["task", "list", "--anon", "--json"]);
    let without = document(dir.path(), &["task", "list", "--json"]);
    let figures = |doc: &Value| -> Vec<(i64, i64)> {
        doc["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|task| (task["id"].as_i64().unwrap(), task["completeness"].as_i64().unwrap()))
            .collect()
    };
    assert_eq!(figures(&with), figures(&without));

    // And the records themselves were not touched.
    assert!(without.to_string().contains("Zebra crossing repaint"));
}

#[test]
#[serial]
fn a_command_that_writes_or_asks_refuses_the_flag_and_does_nothing() {
    let dir = sandbox();
    seed(dir.path());
    let note = dir.path().join("note.md");
    let note_arg = note.to_string_lossy().to_string();

    for args in [
        &["task", "add", "--name", "Never added", "--anon"][..],
        &["task", "show", "--anon"],
        &["task", "remove", "1", "--yes", "--anon"],
        &["report", "--send", "--anon"],
        &["standup", "--append", &note_arg, "--anon"],
        &["inbox", "show", "ZEBRA-77", "--why", "--anon"],
        &["inbox", "pin", "ZEBRA-77", "--anon"],
        &["tag", "show", "zebratag", "--anon"],
        &["template", "show", "zebra-walk", "--anon"],
        &["export", "tasks", "--anon"],
        &["end", "--anon"],
        &["sum", "--anon"],
    ] {
        let out = run(dir.path(), args);
        assert!(!out.status.success(), "kasl {args:?} must refuse --anon");
        assert!(said(&out).contains("--anon is for"), "kasl {args:?}: {}", said(&out));
    }

    assert!(!note.exists(), "the refused standup wrote no note");
    let tasks = document(dir.path(), &["task", "list", "--all", "--json"]);
    assert_eq!(tasks["tasks"].as_array().unwrap().len(), 2, "nothing was added or removed");
}
