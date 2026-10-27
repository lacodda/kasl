//! Output for scripts and logs - `--json`, `--plain`, `NO_COLOR` - run as the
//! binary against a seeded sandbox.
//!
//! What is held here is the contract a reader relies on, not the look:
//! that `--json` leaves exactly one document on stdout whatever else the run
//! has to say; that a command which cannot print JSON refuses rather than
//! answering in prose; that `--plain` and `NO_COLOR` leave no escape code
//! even when the environment forces colour; and that a reader closing the
//! pipe early does not turn a finished command into a crash.

use chrono::{Duration, Local};
use kasl::api::jira::JiraConfig;
use kasl::db::inbox::{Inbox, InboxUpsert};
use kasl::db::workdays::Workdays;
use kasl::libs::config::{Config, InboxConfig};
use serde_json::{Value, json};
use serial_test::serial;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

const ESC: char = '\u{1b}';

/// Environment variables a run sets.
type Env<'a> = &'a [(&'a str, &'a str)];

fn sandbox() -> TempDir {
    let dir = TempDir::new().unwrap();
    // SAFETY: tests touching the env are #[serial]
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("LOCALAPPDATA", dir.path());
    }
    dir
}

/// The binary, in the sandbox, with no colour or debug setting inherited from
/// whoever runs the tests - each test sets the ones it is about.
fn command(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kasl"));
    command.args(args).env("HOME", dir).env("LOCALAPPDATA", dir).stdin(Stdio::null());
    for name in ["NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE", "KASL_DEBUG", "RUST_LOG"] {
        command.env_remove(name);
    }
    for (name, value) in env {
        command.env(name, value);
    }
    command
}

fn run(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    command(dir, args, env).output().expect("kasl runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// Runs a `--json` command and returns its document, failing unless stdout
/// is exactly one JSON object - nothing before it, nothing after it.
fn document(dir: &Path, args: &[&str]) -> Value {
    let out = run(dir, args, &[]);
    assert!(out.status.success(), "kasl {args:?} failed:\n{}{}", stdout(&out), stderr(&out));
    let text = stdout(&out);
    let value: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("kasl {args:?}: stdout is not one JSON document ({error}):\n{text}"));
    assert!(value.is_object(), "kasl {args:?}: the top level must be an object:\n{text}");
    value
}

fn upsert(key: &str, priority: &str, rank: i32, score: f64) -> InboxUpsert {
    InboxUpsert {
        issue_key: key.to_string(),
        issue_id: "1".to_string(),
        summary: format!("Summary for {key}"),
        status_id: Some("10".to_string()),
        status_name: "Open".to_string(),
        priority: Some(priority.to_string()),
        priority_rank: rank,
        sort_value: Some(score),
        url: format!("https://jira.example.com/browse/{key}"),
        raw_updated: None,
        issue_type: Some("Bug".to_string()),
        project: Some("KA".to_string()),
        source: "jira".to_string(),
    }
}

/// A day with something in every corner: tasks with tags, a template, a
/// workday that started hours ago, and an inbox with a pinned, a sleeping
/// and a gone issue.
fn seed(dir: &Path) {
    for args in [
        &["task", "add", "--name", "Ship scripting output", "--tags", "cli,json"][..],
        &["task", "add", "--name", "Second task", "--completeness", "40"][..],
        &["template", "add", "--name", "standup", "--task-name", "Daily standup"][..],
    ] {
        let out = run(dir, args, &[]);
        assert!(out.status.success(), "seeding {args:?}: {}", stderr(&out));
    }

    let today = Local::now().date_naive();
    Workdays::new().unwrap().insert_start(today).unwrap();
    // Hours of work, but never yesterday's: a run just after midnight starts at midnight.
    let start = (Local::now().naive_local() - Duration::hours(3)).max(today.and_hms_opt(0, 0, 1).unwrap());
    let db = kasl::db::db::Db::new().unwrap();
    // Whole seconds, as kasl writes its own timestamps.
    let start = start.format("%Y-%m-%d %H:%M:%S").to_string();
    db.conn.execute("UPDATE workdays SET start = ?1", rusqlite::params![start]).unwrap();

    Inbox::new()
        .unwrap()
        .upsert_batch(&[
            upsert("KA-1", "High", 2, 8.0),
            upsert("KA-2", "Medium", 3, 3.0),
            upsert("KA-3", "Low", 4, 9.0),
            upsert("KA-4", "Highest", 1, 1.0),
        ])
        .unwrap();
    let now = Local::now().naive_local();
    db.conn.execute("UPDATE inbox SET pinned = 1 WHERE issue_key = 'KA-1'", []).unwrap();
    db.conn
        .execute(
            "UPDATE inbox SET snoozed_until = ?1 WHERE issue_key = 'KA-2'",
            rusqlite::params![now + Duration::days(2)],
        )
        .unwrap();
    db.conn
        .execute("UPDATE inbox SET gone_at = ?1 WHERE issue_key = 'KA-3'", rusqlite::params![now])
        .unwrap();
}

#[test]
#[serial]
fn every_reading_command_prints_one_json_document() {
    let dir = sandbox();
    seed(dir.path());

    let commands: &[&[&str]] = &[
        &["report", "--json"],
        &["report", "--last", "--json"],
        &["sum", "--json"],
        &["pauses", "--json"],
        &["pauses", "list", "--json"],
        &["task", "list", "--json"],
        &["task", "list", "--all", "--json"],
        &["task", "list", "--tag", "cli", "--json"],
        &["task", "show", "1", "2", "--json"],
        &["tag", "list", "--json"],
        &["tag", "show", "cli", "--json"],
        &["template", "list", "--json"],
        &["template", "show", "standup", "--json"],
        &["template", "search", "stand", "--json"],
        &["inbox", "--json"],
        &["inbox", "list", "--all", "--snoozed", "--dismissed", "--json"],
        &["inbox", "show", "KA-1", "--why", "--json"],
        &["inbox", "rules", "--json"],
        &["server", "status", "--json"],
        &["server", "queue", "--json"],
        &["server", "pulse", "--json"],
    ];
    for args in commands {
        document(dir.path(), args);
    }

    // Showing one thing is one object; listing is a list.
    let shown = document(dir.path(), &["template", "show", "standup", "--json"]);
    assert_eq!(shown["template"]["task_name"], json!("Daily standup"), "{shown:#}");
    let tagged = document(dir.path(), &["tag", "show", "cli", "--json"]);
    assert_eq!(tagged["tag"]["name"], json!("cli"));
    assert_eq!(tagged["tasks"][0]["name"], json!("Ship scripting output"), "{tagged:#}");
}

#[test]
#[serial]
fn the_day_in_json_adds_up() {
    let dir = sandbox();
    seed(dir.path());

    let day = document(dir.path(), &["report", "--json"]);
    assert_eq!(day["date"], json!(Local::now().date_naive().to_string()));
    assert!(day["workday"]["start"].is_string() && day["workday"]["end"].is_null(), "{day:#}");
    let intervals = day["intervals"].as_array().unwrap();
    assert!(!intervals.is_empty(), "an open day has an interval running to now: {day:#}");
    let sum: i64 = intervals.iter().map(|interval| interval["seconds"].as_i64().unwrap()).sum();
    assert_eq!(day["work_seconds"].as_i64(), Some(sum), "work_seconds is the sum of the intervals");
    assert_eq!(day["tasks"].as_array().unwrap().len(), 2);
    assert_eq!(day["tasks"][0]["tags"], json!(["cli", "json"]));
    assert_eq!(day["tasks"][1]["completeness"], json!(40));
    // KA-1 and KA-4 wait; KA-2 is asleep and KA-3 gone, as the report line counts them.
    assert_eq!(day["inbox"], json!({"total": 2, "fresh": 2, "taken": 0}), "{day:#}");

    let yesterday = document(dir.path(), &["report", "--last", "--json"]);
    assert!(yesterday["workday"].is_null() && yesterday["intervals"] == json!([]), "{yesterday:#}");

    let month = document(dir.path(), &["sum", "--json"]);
    let today = month["days"].as_array().unwrap().iter().find(|d| d["date"] == day["date"]).cloned();
    assert!(today.is_some_and(|d| d["rest_day"] == json!(false)), "{month:#}");
}

#[test]
#[serial]
fn the_inbox_json_is_what_rigger_reads() {
    let dir = sandbox();
    seed(dir.path());

    // The exact call rigger makes (its task page, "The shape rigger reads from kasl").
    let list = document(dir.path(), &["inbox", "list", "--all", "--snoozed", "--json"]);
    let issues = list["issues"].as_array().unwrap();
    let keys: Vec<&str> = issues.iter().map(|issue| issue["key"].as_str().unwrap()).collect();

    // Same issues, same order as the list a person reads.
    let plain = run(dir.path(), &["inbox", "list", "--all", "--snoozed", "--plain"], &[]);
    assert!(plain.status.success(), "{}", stderr(&plain));
    let text = stdout(&plain);
    let header = text.lines().position(|line| line.contains("\tKEY\t")).expect("a header row");
    let key_column = text.lines().nth(header).unwrap().split('\t').position(|cell| cell == "KEY").unwrap();
    let listed: Vec<&str> = text.lines().skip(header + 1).map(|line| line.split('\t').nth(key_column).unwrap()).collect();
    assert_eq!(keys, listed, "the JSON order is the list's order");
    assert_eq!(keys.len(), 4, "--all brings the gone issue, --snoozed the sleeping one");
    assert_eq!(list["total"], json!(4));

    let moment = |value: &Value| {
        let text = value.as_str().unwrap_or_default();
        text.len() == 19 && chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").is_ok()
    };
    for issue in issues {
        for field in [
            "key",
            "summary",
            "status",
            "priority",
            "score",
            "url",
            "first_seen",
            "taken_at",
            "pinned",
            "snoozed_until",
            "gone_at",
        ] {
            assert!(issue.get(field).is_some(), "rigger's field {field} is missing: {issue:#}");
        }
        assert!(moment(&issue["first_seen"]), "moments are YYYY-MM-DDTHH:MM:SS: {issue:#}");
    }
    let by_key = |key: &str| issues.iter().find(|issue| issue["key"] == key).unwrap();
    assert_eq!(by_key("KA-1")["pinned"], json!(true));
    assert!(moment(&by_key("KA-2")["snoozed_until"]));
    assert!(moment(&by_key("KA-3")["gone_at"]));
    // No name learnt for status 10 yet: null, with the id beside it.
    assert!(by_key("KA-4")["taken_at"].is_null() && by_key("KA-4")["status"].is_null() && by_key("KA-4")["status_id"] == json!("10"));

    // A cut list says it is one: the slice and the size of the whole.
    let cut = document(dir.path(), &["inbox", "--priority", "High+", "--json"]);
    assert_eq!(cut["total"], json!(2), "the default list holds KA-1 and KA-4: {cut:#}");
    assert_eq!(cut["filter"], json!(["priority High+"]));

    // The limit cuts the document as it cuts the table: the top of the order.
    let top = document(dir.path(), &["inbox", "-n", "1", "--sort", "priority", "--json"]);
    assert_eq!(top["issues"].as_array().unwrap().len(), 1, "{top:#}");
    assert_eq!(top["issues"][0]["key"], json!("KA-1"), "pinned first, whatever the order: {top:#}");
    let top = document(dir.path(), &["inbox", "list", "--sort", "new", "--json"]);
    assert_eq!(top["issues"].as_array().unwrap().len(), 2, "{top:#}");
}

#[test]
#[serial]
fn json_is_refused_where_it_would_be_dropped() {
    let dir = sandbox();
    seed(dir.path());

    for args in [
        &["inbox", "--json", "pin", "KA-4"][..],
        &["pauses", "--json", "add", "--start", "00:00", "--minutes", "5"][..],
        &["server", "pulse", "--json", "enable"][..],
        &["report", "--send", "--json"][..],
        &["report", "--month", "--json"][..],
        &["sum", "--send", "--json"][..],
        &["task", "add", "--name", "x", "--json"][..],
        &["end", "--json"][..],
    ] {
        let out = run(dir.path(), args, &[]);
        assert!(!out.status.success(), "kasl {args:?} accepted --json");
        assert!(stdout(&out).trim().is_empty(), "kasl {args:?} printed to stdout:\n{}", stdout(&out));
    }
    // Refused before acting: nothing was pinned.
    let issue = document(dir.path(), &["inbox", "show", "KA-4", "--json"]);
    assert_eq!(issue["issue"]["pinned"], json!(false));
}

#[test]
#[serial]
fn a_named_thing_that_is_not_there_fails() {
    let dir = sandbox();
    seed(dir.path());

    for args in [
        &["task", "show", "999", "--json"][..],
        &["tag", "show", "nope", "--json"][..],
        &["template", "show", "nope", "--json"][..],
        &["inbox", "show", "NOPE-1", "--json"][..],
        &["inbox", "show", "NOPE-1"][..],
    ] {
        let out = run(dir.path(), args, &[]);
        assert!(!out.status.success(), "kasl {args:?} succeeded on nothing");
        assert!(stdout(&out).trim().is_empty(), "kasl {args:?}:\n{}", stdout(&out));
    }
}

#[test]
#[serial]
fn plain_is_tab_separated_and_marked_with_words() {
    let dir = sandbox();
    seed(dir.path());

    let out = run(dir.path(), &["task", "list", "--plain"], &[]);
    let text = stdout(&out);
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("#\tID\tTASK ID\tNAME\tDONE\tTAGS"), "{text}");
    assert_eq!(lines.next(), Some("1\t1\t1\tShip scripting output\t100%\tcli, json"), "{text}");

    // A cell is never cut: a log keeps the whole fact.
    let long = "A task name long enough that any terminal table would have to cut it somewhere well before its very last word, which is here";
    assert!(run(dir.path(), &["task", "add", "--name", long], &[]).status.success());
    let text = stdout(&run(dir.path(), &["task", "list", "--plain"], &[]));
    assert!(text.contains(long), "{text}");
    assert!(!text.contains('…'), "{text}");

    // No blank lines around headings: a pipe would read them as empty records.
    let text = stdout(&run(dir.path(), &["report", "--plain"], &[]));
    assert!(!text.is_empty() && text.lines().all(|line| !line.trim().is_empty()), "{text:?}");

    let failed = run(dir.path(), &["inbox", "show", "NOPE-1", "--plain"], &[]);
    assert!(stderr(&failed).starts_with("error: "), "{}", stderr(&failed));
    let failed = run(dir.path(), &["inbox", "show", "NOPE-1"], &[]);
    assert!(stderr(&failed).starts_with("❌ "), "a person still gets the mark: {}", stderr(&failed));
}

#[test]
#[serial]
fn no_escape_code_survives_plain_or_no_color() {
    let dir = sandbox();
    seed(dir.path());
    let force = ("CLICOLOR_FORCE", "1");

    // The probe: forced colour does reach the output, so its absence below
    // is the switch working and not a test that cannot see colour at all.
    let help = run(dir.path(), &["--help"], &[force]);
    assert!(stdout(&help).contains(ESC), "CLICOLOR_FORCE did not colour clap's help");
    // No workday yesterday: the "not found" line goes through the debug log.
    let debug = run(dir.path(), &["report", "--last"], &[force, ("KASL_DEBUG", "1")]);
    assert!(stderr(&debug).contains(ESC), "CLICOLOR_FORCE did not colour the debug log");

    let quiet: &[(&[&str], Env)] = &[
        (&["--help", "--plain"], &[force]),
        (&["task", "--plain", "--help"], &[force]),
        (&["--help"], &[force, ("NO_COLOR", "1")]),
        (&["report", "--last"], &[force, ("KASL_DEBUG", "1"), ("NO_COLOR", "1")]),
        (&["report", "--last", "--plain"], &[force, ("KASL_DEBUG", "1")]),
        (&["report", "--last", "--json"], &[force, ("KASL_DEBUG", "1")]),
        (&["report", "--plain"], &[force]),
        (&["inbox", "--plain"], &[force]),
        (&["nonsense", "--plain"], &[force]),
    ];
    for (args, env) in quiet {
        let out = run(dir.path(), args, env);
        assert!(
            !stdout(&out).contains(ESC) && !stderr(&out).contains(ESC),
            "kasl {args:?} with {env:?} wrote an escape code:\n{}{}",
            stdout(&out),
            stderr(&out)
        );
    }
}

#[test]
#[serial]
fn json_stays_parseable_in_debug_mode() {
    let dir = sandbox();
    seed(dir.path());
    skip_a_rule();

    // The skipped rule's warning goes through the debug log - which must be
    // on stderr, or it lands in front of the document.
    let out = run(dir.path(), &["inbox", "--json"], &[("KASL_DEBUG", "1")]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("skipped"), "the probe: the debug log said nothing:\n{}", stderr(&out));
    serde_json::from_slice::<Value>(&out.stdout).unwrap_or_else(|e| panic!("the debug log reached stdout ({e}):\n{}", stdout(&out)));
}

/// Configures one rule with a typo, which kasl skips and warns about over every list.
fn skip_a_rule() {
    let rules = json!([{"when": {"prority": "High"}, "then": {"dismiss": true}}]);
    Config {
        jira: Some(JiraConfig {
            login: "agent".to_string(),
            api_url: "https://jira.example.com".to_string(),
            completed_statuses: Vec::new(),
        }),
        inbox: Some(InboxConfig {
            notify: false,
            rules: rules.as_array().cloned().unwrap(),
            ..Default::default()
        }),
        ..Default::default()
    }
    .save()
    .unwrap();
}

#[test]
#[serial]
fn warnings_stay_off_stdout() {
    let dir = sandbox();
    seed(dir.path());
    skip_a_rule();

    let plain = run(dir.path(), &["inbox", "--plain"], &[]);
    assert!(plain.status.success(), "{}", stderr(&plain));
    assert!(stderr(&plain).contains("warning: "), "the warning is gone: {}", stderr(&plain));
    assert!(!stdout(&plain).contains("warning"), "the warning is between the rows:\n{}", stdout(&plain));

    let person = run(dir.path(), &["inbox"], &[]);
    assert!(stderr(&person).contains("⚠️ ") && !stdout(&person).contains("⚠️"), "{}", stdout(&person));

    let rules = document(dir.path(), &["inbox", "rules", "--json"]);
    assert_eq!(rules["skipped"][0]["number"], json!(1), "{rules:#}");
    document(dir.path(), &["inbox", "--json"]);
}

#[test]
#[serial]
fn a_reader_that_leaves_early_is_not_a_crash() {
    let dir = sandbox();
    seed(dir.path());
    // Enough rows to outrun any pipe buffer.
    let db = kasl::db::db::Db::new().unwrap();
    let tx = db.conn.unchecked_transaction().unwrap();
    for n in 0..3000 {
        tx.execute(
            "INSERT INTO tasks (name, comment, completeness) VALUES (?1, '', 100)",
            rusqlite::params![format!("Row {n}")],
        )
        .unwrap();
    }
    tx.commit().unwrap();

    for args in [&["task", "list", "--all", "--plain"][..], &["completions", "bash"][..]] {
        let mut child = command(dir.path(), args, &[]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        // Read nothing and hang up, as `| head -0` would.
        drop(child.stdout.take());
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "kasl {args:?} ended {:?}:\n{}", out.status, stderr(&out));
        assert!(!stderr(&out).contains("panicked"), "{}", stderr(&out));
    }
}
