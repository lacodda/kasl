//! `kasl demo`, run as the binary against a sandbox.
//!
//! What is held here is what the demo promises: that it is built on the
//! first word, that every reading command has something to show on it, that
//! it is a place of its own - nothing of the person's is read or written
//! from inside it, and nothing of the demo's from outside - and that the
//! commands which act beyond a data directory are refused there.

use kasl::db::tasks::Tasks;
use kasl::libs::task::Task;
use serde_json::Value;
use serial_test::serial;
use std::path::{Path, PathBuf};
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

fn ok(dir: &Path, args: &[&str]) -> Output {
    let out = run(dir, args);
    assert!(out.status.success(), "kasl {args:?} failed:\n{}{}", stdout(&out), stderr(&out));
    out
}

fn document(dir: &Path, args: &[&str]) -> Value {
    let text = stdout(&ok(dir, args));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("kasl {args:?}: not a JSON document ({error}):\n{text}"))
}

/// The real data directory of the sandbox, wherever this platform keeps it.
fn data_dir(_dir: &Path) -> PathBuf {
    kasl::libs::data_storage::DataStorage::new().base().to_path_buf()
}

fn task_names(document: &Value) -> Vec<String> {
    document["tasks"]
        .as_array()
        .expect("a list of tasks")
        .iter()
        .map(|task| task["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
#[serial]
fn the_first_word_builds_it_in_a_place_of_its_own_and_says_what_to_try() {
    let dir = sandbox();
    let out = ok(dir.path(), &["demo"]);
    let said = stdout(&out);

    assert!(said.contains("The demo is built"), "{said}");
    for command in ["kasl demo report", "kasl demo sum", "kasl demo inbox"] {
        assert!(said.contains(command), "the introduction names `{command}`:\n{said}");
    }
    // A database being made is not one being upgraded: the introduction is
    // not buried under a line per migration.
    assert!(!stderr(&out).contains("migration"), "{}", stderr(&out));

    let data = data_dir(dir.path());
    assert!(data.join("demo").join("kasl.db").exists(), "the demo has its own database");
    assert!(!data.join("kasl.db").exists(), "and building it made no database for the person's own records");

    // Asked again on the same day, it is the same demo.
    assert!(stdout(&ok(dir.path(), &["demo"])).contains("The demo is ready"));
}

#[test]
#[serial]
fn a_command_after_the_word_builds_the_demo_too_and_every_reading_command_shows_something() {
    let dir = sandbox();

    // The very first command a newcomer types may be this one.
    let tasks = document(dir.path(), &["demo", "task", "list", "--all", "--json"]);
    assert!(task_names(&tasks).len() > 60, "eight weeks of daily rows");
    assert!(task_names(&tasks).iter().any(|name| name.starts_with("ORBIT-")));

    // Last week is whole whatever day the suite runs on.
    let week = stdout(&ok(dir.path(), &["demo", "standup", "--week", "--last"]));
    assert!(week.contains("Worked ") && week.contains("**Done**"), "{week}");
    // An issue the inbox still knows - in hand, or resolved lately - is linked.
    let weeks = format!("{week}{}", stdout(&ok(dir.path(), &["demo", "standup", "--week"])));
    assert!(weeks.contains("](https://tracker.example/browse/ORBIT-"), "{weeks}");

    let inbox = document(dir.path(), &["demo", "inbox", "list", "--json"]);
    let issues = inbox["issues"].as_array().unwrap();
    assert!(issues.len() >= 7, "{inbox}");
    assert!(issues.iter().any(|issue| issue["pinned"] == true));
    assert_eq!(issues.iter().filter(|issue| issue["badge"] == "NEW").count(), 3);
    let everything = document(dir.path(), &["demo", "inbox", "list", "--all", "--snoozed", "--dismissed", "--json"]);
    let badges: Vec<&str> = everything["issues"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|issue| issue["badge"].as_str())
        .collect();
    for badge in ["dismissed", "resolved"] {
        assert!(badges.contains(&badge), "the inbox shows a {badge} issue: {badges:?}");
    }
    assert!(badges.iter().any(|badge| badge.starts_with("zzz ")), "{badges:?}");

    assert_eq!(document(dir.path(), &["demo", "tag", "list", "--json"])["tags"].as_array().unwrap().len(), 7);
    assert_eq!(
        document(dir.path(), &["demo", "template", "list", "--json"])["templates"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    // The rest read the same tables; each has to run on what the generator wrote.
    for args in [
        &["demo", "report"][..],
        &["demo", "report", "--last", "--json"],
        &["demo", "sum"],
        &["demo", "sum", "--json"],
        &["demo", "status"],
        &["demo", "status", "--json"],
        &["demo", "standup"],
        &["demo", "pauses"],
        &["demo", "pauses", "list", "--json"],
        &["demo", "inbox"],
        &["demo", "task", "list"],
        &["--plain", "demo", "inbox"],
    ] {
        ok(dir.path(), args);
    }
}

#[test]
#[serial]
fn the_demo_and_the_real_records_never_meet() {
    let dir = sandbox();
    Tasks::new().unwrap().insert(&Task::new("A real task of mine", "", Some(50))).unwrap();

    let in_demo = task_names(&document(dir.path(), &["demo", "task", "list", "--all", "--json"]));
    assert!(!in_demo.iter().any(|name| name.contains("real task")), "the demo reads nothing of the person's");

    ok(dir.path(), &["demo", "task", "add", "--name", "Tried in the demo", "--completeness", "10"]);
    let in_demo = task_names(&document(dir.path(), &["demo", "task", "list", "--all", "--json"]));
    assert!(
        in_demo.iter().any(|name| name == "Tried in the demo"),
        "what is added to the demo stays through the day"
    );

    let real = task_names(&document(dir.path(), &["task", "list", "--all", "--json"]));
    assert_eq!(real, vec!["A real task of mine"], "and never reaches the person's own records");
}

#[test]
#[serial]
fn a_demo_from_another_day_is_made_anew_and_reset_does_it_on_request() {
    let dir = sandbox();
    ok(dir.path(), &["demo"]);
    let has_it = |dir: &Path| task_names(&document(dir, &["demo", "task", "list", "--all", "--json"])).contains(&"Tried in the demo".to_string());

    ok(dir.path(), &["demo", "task", "add", "--name", "Tried in the demo"]);
    assert!(has_it(dir.path()));
    ok(dir.path(), &["demo", "--reset"]);
    assert!(!has_it(dir.path()), "--reset drops what was added");

    // A demo built on another day has no today in it.
    ok(dir.path(), &["demo", "task", "add", "--name", "Tried in the demo"]);
    let marker = data_dir(dir.path()).join("demo").join("demo.json");
    let mut written: Value = serde_json::from_str(&std::fs::read_to_string(&marker).unwrap()).unwrap();
    written["built"] = Value::String("2020-01-01".to_string());
    std::fs::write(&marker, written.to_string()).unwrap();

    let out = ok(dir.path(), &["demo", "status"]);
    assert!(stderr(&out).contains("made anew"), "said beside the output: {}", stderr(&out));
    assert!(!stdout(&out).contains("made anew"), "and never in it: {}", stdout(&out));
    assert!(!has_it(dir.path()));
    let rebuilt: Value = serde_json::from_str(&std::fs::read_to_string(&marker).unwrap()).unwrap();
    assert_eq!(rebuilt["built"], chrono::Local::now().date_naive().to_string());
}

#[test]
#[serial]
fn what_acts_beyond_the_data_directory_is_refused_in_the_demo() {
    let dir = sandbox();
    for (args, word) in [
        (&["demo", "watch"][..], "kasl watch"),
        (&["demo", "watch", "--foreground"], "kasl watch"),
        (&["demo", "autostart", "status"], "kasl autostart"),
        (&["demo", "server", "status"], "kasl server"),
        (&["demo", "self-update"], "kasl self-update"),
    ] {
        let out = run(dir.path(), args);
        assert!(!out.status.success(), "kasl {args:?} must be refused");
        assert!(stderr(&out).contains(word), "kasl {args:?} names the command to run instead: {}", stderr(&out));
    }
    // A refusal builds nothing and starts nothing.
    assert!(!data_dir(dir.path()).join("demo").join("kasl.pid").exists());

    // A flag between the word and the command is not a command on the demo.
    let out = run(dir.path(), &["demo", "--reset", "report"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("kasl demo report"), "{}", stderr(&out));
}
