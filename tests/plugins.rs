//! The plugin protocol, run the way kasl runs it: a real `kasl` process
//! calling a real plugin process.
//!
//! The fixture plugin is a script - a `.cmd` on Windows, `sh` elsewhere -
//! that prints a manifest it is given, appends every invocation it receives
//! to a file, and answers with a reply the test sets. What kasl sends is
//! read back from that file, so each test checks the wire, not a mock of it.
//!
//! Every `kasl` here runs with `PATH` cut down to the system's own tools:
//! discovery searches `PATH`, and a plugin the machine really has installed
//! must never be run by a test - an event would send real mail.

use chrono::{Duration, Local};
use kasl::db::inbox::{Inbox, InboxUpsert};
use kasl::db::tasks::Tasks;
use kasl::db::workdays::Workdays;
use kasl::libs::config::{Config, PluginConfig, ProductivityConfig};
use kasl::libs::task::Task;
use serde_json::{Value, json};
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn sandbox() -> TempDir {
    let dir = TempDir::new().unwrap();
    // SAFETY: every test here is #[serial].
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("LOCALAPPDATA", dir.path());
    }
    dir
}

/// The system's own tools and nothing else: `cmd` and `findstr`, or `sh` and `cat`.
fn system_path() -> String {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        format!("{root}\\System32")
    }
    #[cfg(not(windows))]
    {
        "/usr/bin:/bin".to_string()
    }
}

fn kasl(dir: &Path, args: &[&str]) -> Output {
    kasl_at(Path::new(env!("CARGO_BIN_EXE_kasl")), dir, args)
}

/// `kasl` run from `exe` - a copy of the binary somewhere of the test's choosing.
fn kasl_at(exe: &Path, dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(exe);
    command
        .args(args)
        .env("HOME", dir)
        .env("LOCALAPPDATA", dir)
        .env("PATH", system_path())
        .stdin(Stdio::null());
    for name in ["NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE", "KASL_DEBUG", "RUST_LOG"] {
        command.env_remove(name);
    }
    command.output().expect("kasl runs")
}

fn text(out: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn ok(dir: &Path, args: &[&str]) -> String {
    let out = kasl(dir, args);
    assert!(out.status.success(), "kasl {args:?} failed:\n{}", text(&out));
    text(&out)
}

fn document(dir: &Path, args: &[&str]) -> Value {
    let out = kasl(dir, args);
    assert!(out.status.success(), "kasl {args:?} failed:\n{}", text(&out));
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("kasl {args:?}: not JSON ({e}):\n{}", text(&out)))
}

/// A plugin installed in kasl's own plugins directory.
struct Fixture {
    calls: PathBuf,
    reply: PathBuf,
}

impl Fixture {
    /// Installs `kasl-plugin-NAME` in kasl's plugins directory, describing
    /// itself with `manifest`, answering every call with `{}` and exiting
    /// with `code`.
    fn install(name: &str, manifest: Value, code: i32) -> Fixture {
        Self::install_in(&kasl::libs::plugin::plugins_dir().unwrap(), name, manifest, code)
    }

    /// The same, in a directory of the test's choosing.
    fn install_in(dir: &Path, name: &str, manifest: Value, code: i32) -> Fixture {
        std::fs::create_dir_all(dir).unwrap();
        let manifest_path = dir.join(format!("{name}.manifest.json"));
        std::fs::write(&manifest_path, manifest.to_string()).unwrap();
        let fixture = Fixture {
            calls: dir.join(format!("{name}.calls.jsonl")),
            reply: dir.join(format!("{name}.reply.json")),
        };
        fixture.answer(json!({}));

        #[cfg(windows)]
        {
            // Printed from files beside the script: cmd's `echo` mangles
            // quotes, and this is about testing kasl, not batch escaping.
            let script = format!(
                "@echo off\r\nif \"%~1\"==\"--manifest\" (\r\n  type \"{}\"\r\n  exit /b 0\r\n)\r\nfindstr \"^\" >> \"{}\"\r\ntype \"{}\"\r\n{}exit /b {code}\r\n",
                manifest_path.display(),
                fixture.calls.display(),
                fixture.reply.display(),
                if code != 0 { "echo it broke 1>&2\r\n" } else { "" },
            );
            std::fs::write(dir.join(format!("kasl-plugin-{name}.cmd")), script).unwrap();
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.join(format!("kasl-plugin-{name}"));
            let script = format!(
                "#!/bin/sh\nif [ \"$1\" = \"--manifest\" ]; then\n  cat '{}'\n  exit 0\nfi\ncat >> '{}'\ncat '{}'\n{}exit {code}\n",
                manifest_path.display(),
                fixture.calls.display(),
                fixture.reply.display(),
                if code != 0 { "echo it broke >&2\n" } else { "" },
            );
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        fixture
    }

    /// What the plugin answers from now on.
    fn answer(&self, reply: Value) {
        std::fs::write(&self.reply, reply.to_string()).unwrap();
    }

    /// Every invocation it has received, in order.
    fn calls(&self) -> Vec<Value> {
        let Ok(written) = std::fs::read_to_string(&self.calls) else {
            return Vec::new();
        };
        written
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("not one invocation per line ({e}): {line}")))
            .collect()
    }
}

fn manifest(extra: Value) -> Value {
    let mut manifest = json!({"protocol_version": 1, "name": "demo", "version": "0.1.0", "description": "A plugin for tests"});
    manifest.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    manifest
}

fn set_settings(name: &str, settings: Value) {
    let mut config = Config::read().unwrap();
    let Value::Object(settings) = settings else { panic!() };
    config.plugins.insert(name.to_string(), PluginConfig { settings });
    config.save().unwrap();
}

/// A day started two hours ago, with one task.
fn a_working_day() {
    let today = Local::now().date_naive();
    let mut workdays = Workdays::new().unwrap();
    workdays.insert_start(today).unwrap();
    workdays.update_start(today, Local::now().naive_local() - Duration::hours(2)).unwrap();
    Tasks::new().unwrap().insert(&Task::new("Write the plugin protocol", "", Some(100))).unwrap();
}

fn listed(key: &str, summary: &str) -> Value {
    json!({"key": key, "summary": summary, "url": format!("https://tracker.example.com/{key}"), "status": "Opened"})
}

fn inbox(dir: &Path) -> Vec<Value> {
    document(dir, &["inbox", "list", "--all", "--json"])["issues"].as_array().unwrap().clone()
}

#[test]
#[serial]
fn a_plugin_is_listed_with_what_it_offers_and_what_it_needs() {
    let dir = sandbox();
    Fixture::install(
        "demo",
        manifest(json!({
            "commands": [{"key": "mail", "label": "Mail the day", "target": "day"}],
            "events": ["workday_end"],
            "requests": ["list_issues"],
            "settings": [{"key": "url", "label": "URL", "required": true}, {"key": "token", "label": "Token", "secret": true}]
        })),
        0,
    );

    let listed = document(dir.path(), &["plugin", "list", "--json"]);
    let demo = &listed["plugins"][0];
    assert_eq!(demo["name"], json!("demo"));
    assert_eq!(demo["state"], json!("needs_setup"));
    assert_eq!(demo["missing"], json!(["url"]));
    assert_eq!(demo["manifest"]["events"], json!(["workday_end"]));

    set_settings("demo", json!({"url": "https://tracker.example.com"}));
    let listed = document(dir.path(), &["plugin", "list", "--json"]);
    assert_eq!(
        listed["plugins"][0]["state"],
        json!("ready"),
        "an optional secret left unset does not hold it back"
    );

    let words = ok(dir.path(), &["plugin", "list"]);
    assert!(words.contains("demo 0.1.0 - A plugin for tests") && words.contains("ready"), "{words}");
    assert!(words.contains("command  mail (on a day)") && words.contains("hears    workday_end"), "{words}");
}

#[test]
#[serial]
fn a_plugin_that_cannot_describe_itself_is_listed_with_the_reason() {
    let dir = sandbox();
    Fixture::install("future", json!({"protocol_version": 9, "name": "future", "version": "1"}), 0);
    Fixture::install("broken", json!({}), 0);
    // The script prints whatever its manifest file holds.
    std::fs::write(kasl::libs::plugin::plugins_dir().unwrap().join("broken.manifest.json"), "not json").unwrap();

    let listed = document(dir.path(), &["plugin", "list", "--json"]);
    let by_name = |name: &str| listed["plugins"].as_array().unwrap().iter().find(|p| p["name"] == json!(name)).unwrap().clone();

    assert_eq!(by_name("future")["state"], json!("unusable"));
    assert!(by_name("future")["problem"].as_str().unwrap().contains("speaks protocol 9"));
    assert_eq!(by_name("broken")["state"], json!("unusable"));
    assert!(by_name("broken")["problem"].as_str().unwrap().contains("could not be read"));
}

#[test]
#[serial]
fn a_command_is_handed_the_day_whole_and_its_settings() {
    let dir = sandbox();
    let demo = Fixture::install(
        "demo",
        manifest(json!({
            "commands": [{"key": "mail", "label": "Mail the day", "target": "day"}],
            "settings": [{"key": "to", "label": "Recipient", "required": true}, {"key": "stale", "label": "Old"}]
        })),
        0,
    );
    a_working_day();

    let refused = kasl(dir.path(), &["plugin", "run", "demo", "mail"]);
    assert!(!refused.status.success(), "a plugin not set up is not run");
    assert!(text(&refused).contains("kasl plugin setup demo"), "{}", text(&refused));
    assert!(demo.calls().is_empty());

    set_settings("demo", json!({"to": "me@example.com"}));
    demo.answer(json!({"message": "Mailed the day", "meta": {"sent_to": "me@example.com"}}));
    let said = ok(dir.path(), &["plugin", "run", "demo", "mail"]);
    assert!(said.contains("demo: Mailed the day") && said.contains("sent_to: me@example.com"), "{said}");

    let call = &demo.calls()[0];
    assert_eq!(call["protocol_version"], json!(1));
    assert_eq!((&call["command"], &call["target"]), (&json!("mail"), &json!("day")));
    assert_eq!(call["subject"]["date"], json!(Local::now().date_naive().to_string()));
    assert_eq!(call["subject"]["tasks"][0]["name"], json!("Write the plugin protocol"));
    assert_eq!(call["settings"], json!({"to": "me@example.com"}), "only what is set, and only what is declared");

    let reply = document(dir.path(), &["plugin", "run", "demo", "mail", "--json"]);
    assert_eq!(reply["reply"]["meta"]["sent_to"], json!("me@example.com"));
}

#[test]
#[serial]
fn ending_the_day_tells_the_plugins_that_hear_it() {
    let dir = sandbox();
    let hears = Fixture::install("hears", manifest(json!({"events": ["workday_end"]})), 0);
    let deaf = Fixture::install("deaf", manifest(json!({"events": ["report_ready"]})), 0);
    let broken = Fixture::install("broken", manifest(json!({"events": ["workday_end"]})), 3);
    hears.answer(json!({"message": "Noted the end"}));
    a_working_day();

    let said = ok(dir.path(), &["end"]);

    assert!(said.contains("hears: Noted the end"), "{said}");
    assert!(
        said.contains("workday_end was not taken") && said.contains("it broke"),
        "a failing listener is said out loud:\n{said}"
    );
    let call = &hears.calls()[0];
    assert_eq!(call["event"], json!("workday_end"));
    assert!(
        call["subject"]["workday"]["end"].is_string(),
        "the day is handed over with its end in place: {call}"
    );
    assert!(
        call.get("command").is_none() && call.get("request").is_none(),
        "an invocation is one kind of call"
    );
    assert!(deaf.calls().is_empty(), "a plugin hears only what it listens to");
    assert_eq!(broken.calls().len(), 1);
}

#[test]
#[serial]
fn sending_the_day_hands_it_to_report_ready_without_siserver() {
    let dir = sandbox();
    let mut config = Config::read().unwrap();
    config.productivity = Some(ProductivityConfig {
        min_productivity_threshold: 0.0,
        workday_hours: 8.0,
        min_workday_fraction_before_suggest: 0.5,
    });
    config.save().unwrap();

    // Nowhere to go: refused before the day is closed.
    a_working_day();
    let refused = kasl(dir.path(), &["report", "--send"]);
    assert!(!refused.status.success());
    assert!(text(&refused).contains("Nowhere to send the day"), "{}", text(&refused));
    let today = Local::now().date_naive();
    assert!(
        Workdays::new().unwrap().fetch(today).unwrap().unwrap().end.is_none(),
        "a send with nowhere to go closes nothing"
    );

    let mail = Fixture::install("mail", manifest(json!({"events": ["report_ready", "workday_end"]})), 0);
    let preview = ok(dir.path(), &["report", "--send", "--show"]);
    assert!(preview.contains("mail"), "the preview names where the day goes:\n{preview}");
    assert!(mail.calls().is_empty(), "a preview sends nothing");

    ok(dir.path(), &["report", "--send"]);

    let events: Vec<Value> = mail.calls().iter().map(|call| call["event"].clone()).collect();
    assert_eq!(events, vec![json!("workday_end"), json!("report_ready")], "the day closes, then it is ready");
    assert_eq!(mail.calls()[1]["subject"]["tasks"][0]["name"], json!("Write the plugin protocol"));
}

#[test]
#[serial]
fn a_plugin_source_fills_the_inbox_and_judges_only_its_own_rows() {
    let dir = sandbox();
    let tracker = Fixture::install("tracker", manifest(json!({"requests": ["list_issues", "transition"]})), 0);
    Inbox::new()
        .unwrap()
        .upsert_batch(&[InboxUpsert {
            issue_key: "KA-1".into(),
            issue_id: "1".into(),
            summary: "A Jira issue".into(),
            status_id: None,
            status_name: String::new(),
            priority: None,
            priority_rank: 999,
            sort_value: None,
            url: "https://jira.example.com/browse/KA-1".into(),
            raw_updated: None,
            issue_type: None,
            project: None,
            source: "jira".into(),
        }])
        .unwrap();

    tracker.answer(json!({"issues": [listed("app#1", "Fix the login"), listed("KA-1", "Claimed by the wrong source")]}));
    let said = ok(dir.path(), &["inbox", "sync"]);
    assert!(said.contains("tracker synced: 2 fetched"), "{said}");
    assert!(said.contains("KA-1 (jira)"), "a key another source holds is named:\n{said}");
    let rows = inbox(dir.path());
    let row = |key: &str| rows.iter().find(|issue| issue["key"] == json!(key)).unwrap().clone();
    assert_eq!(row("app#1")["source"], json!("tracker"));
    assert_eq!(row("app#1")["status"], json!("Opened"));
    assert_eq!(row("KA-1")["source"], json!("jira"));
    assert_eq!(row("KA-1")["summary"], json!("A Jira issue"), "a refused key is left as it was");

    tracker.answer(json!({"issues": []}));
    let said = ok(dir.path(), &["inbox", "sync"]);
    assert!(
        said.contains("0 fetched, 0 new, 0 changed, 1 gone"),
        "only its own issue is counted as gone:\n{said}"
    );
    let rows = inbox(dir.path());
    let row = |key: &str| rows.iter().find(|issue| issue["key"] == json!(key)).unwrap().clone();
    assert!(row("app#1")["gone_at"].is_string(), "its own issue left its poll");
    assert!(row("KA-1")["gone_at"].is_null(), "another source's issue is not this poll's to judge");
    assert_eq!(tracker.calls()[0]["request"], json!("list_issues"));
}

#[test]
#[serial]
fn a_reply_kasl_cannot_read_whole_marks_nothing_gone() {
    let dir = sandbox();
    let tracker = Fixture::install("tracker", manifest(json!({"requests": ["list_issues"]})), 0);
    tracker.answer(json!({"issues": [listed("app#1", "One")]}));
    ok(dir.path(), &["inbox", "sync"]);

    for broken in [json!({"issues": [{"key": "app#2"}]}), json!({}), json!({"error": "the tracker is down"})] {
        tracker.answer(broken.clone());
        let out = kasl(dir.path(), &["inbox", "sync"]);
        assert!(!out.status.success(), "{broken} must fail the sync:\n{}", text(&out));
        assert!(text(&out).contains("tracker did not sync"), "{}", text(&out));
        assert!(inbox(dir.path())[0]["gone_at"].is_null(), "{broken} must not read as an empty list");
    }
}

#[test]
#[serial]
fn an_arrival_is_news_but_the_first_poll_is_not() {
    let dir = sandbox();
    let tracker = Fixture::install("tracker", manifest(json!({"requests": ["list_issues"]})), 0);
    let news = Fixture::install("news", manifest(json!({"events": ["new_inbox_issue"]})), 0);

    tracker.answer(json!({"issues": [listed("app#1", "Already there")]}));
    ok(dir.path(), &["inbox", "sync"]);
    assert!(
        news.calls().is_empty(),
        "the first poll is where kasl starts watching, not a moment issues arrived"
    );

    tracker.answer(json!({"issues": [listed("app#1", "Already there"), listed("app#2", "Just arrived")]}));
    ok(dir.path(), &["inbox", "sync"]);
    let calls = news.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0]["event"], json!("new_inbox_issue"));
    assert_eq!(calls[0]["subject"]["key"], json!("app#2"));
    assert_eq!(calls[0]["subject"]["source"], json!("tracker"));
}

#[test]
#[serial]
fn starting_an_issue_goes_to_the_source_that_listed_it() {
    let dir = sandbox();
    let tracker = Fixture::install("tracker", manifest(json!({"requests": ["list_issues", "transition"]})), 0);
    tracker.answer(json!({"issues": [listed("app#1", "Fix the login")]}));
    ok(dir.path(), &["inbox", "sync"]);

    tracker.answer(json!({"status": "Doing", "moved": true}));
    let said = ok(dir.path(), &["inbox", "start", "app#1"]);
    assert!(said.contains("app#1 is now Doing in tracker"), "{said}");
    let call = tracker.calls().last().unwrap().clone();
    assert_eq!((&call["request"], &call["to"]), (&json!("transition"), &json!("start")));
    assert_eq!(call["subject"]["key"], json!("app#1"));
    assert_eq!(inbox(dir.path())[0]["status"], json!("Doing"), "the status the source answered is kept");

    let refused = kasl(dir.path(), &["inbox", "comment", "app#1", "Looks good"]);
    assert!(!refused.status.success());
    assert!(text(&refused).contains("does not answer `comment`"), "{}", text(&refused));
}

#[test]
#[serial]
fn a_plugin_that_does_not_answer_in_time_is_stopped() {
    let dir = sandbox();
    let plugins = kasl::libs::plugin::plugins_dir().unwrap();
    std::fs::create_dir_all(&plugins).unwrap();
    let manifest = manifest(json!({"commands": [{"key": "wait", "label": "Wait", "target": "day", "timeout": 1}]}));
    let manifest_path = plugins.join("slow.manifest.json");
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
    #[cfg(windows)]
    std::fs::write(
        plugins.join("kasl-plugin-slow.cmd"),
        format!(
            "@echo off\r\nif \"%~1\"==\"--manifest\" (\r\n  type \"{}\"\r\n  exit /b 0\r\n)\r\nping -n 30 127.0.0.1 >NUL\r\n",
            manifest_path.display()
        ),
    )
    .unwrap();
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = plugins.join("kasl-plugin-slow");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--manifest\" ]; then cat '{}'; exit 0; fi\nsleep 30\n",
                manifest_path.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let started = std::time::Instant::now();
    let out = kasl(dir.path(), &["plugin", "run", "slow", "wait"]);

    assert!(!out.status.success());
    assert!(text(&out).contains("did not answer within 1 seconds"), "{}", text(&out));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "stopped at its allowance, not when it gave up"
    );
}

#[test]
#[serial]
fn nothing_installed_says_where_kasl_looks() {
    let dir = sandbox();

    let said = ok(dir.path(), &["plugin", "list"]);
    assert!(said.contains("No plugins installed") && said.contains("kasl-plugin-*"), "{said}");
    let listed = document(dir.path(), &["plugin", "list", "--json"]);
    assert_eq!(listed["plugins"], json!([]));
}

/// kasl's own binary, linked (or copied) into `bin`: a kasl whose directory
/// the test owns, as an installer leaves one.
fn kasl_in(bin: &Path) -> PathBuf {
    std::fs::create_dir_all(bin).unwrap();
    let copy = bin.join(format!("kasl{}", std::env::consts::EXE_SUFFIX));
    if std::fs::hard_link(env!("CARGO_BIN_EXE_kasl"), &copy).is_err() {
        std::fs::copy(env!("CARGO_BIN_EXE_kasl"), &copy).unwrap();
    }
    copy
}

#[test]
#[serial]
fn a_plugin_beside_kasl_is_found_and_one_in_the_plugins_directory_wins() {
    // The release archive carries its plugins beside the binary, and the
    // installers put them there - whether or not that directory is on PATH.
    let dir = sandbox();
    let bin = dir.path().join("bin");
    let exe = kasl_in(&bin);
    Fixture::install_in(&bin, "boxed", manifest(json!({"version": "1.0.0"})), 0);

    let listed = |dir: &Path| {
        let out = kasl_at(&exe, dir, &["plugin", "list", "--json"]);
        assert!(out.status.success(), "{}", text(&out));
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["plugins"][0].clone()
    };
    let boxed = listed(dir.path());
    assert_eq!(boxed["name"], json!("boxed"));
    assert!(boxed["path"].as_str().unwrap().starts_with(&*bin.to_string_lossy()), "{boxed}");

    // Someone put a copy of their own in the plugins directory: it is the one used.
    Fixture::install("boxed", manifest(json!({"version": "2.0.0"})), 0);
    let mine = listed(dir.path());
    assert_eq!(mine["manifest"]["version"], json!("2.0.0"), "{mine}");
    assert!(!mine["path"].as_str().unwrap().starts_with(&*bin.to_string_lossy()));
}

#[test]
#[serial]
fn a_gitlab_reference_is_a_key_like_any_other() {
    // `#`, `!` and `/` reach the database, the command line and the JSON
    // documents as they are.
    let dir = sandbox();
    let source = Fixture::install("forge", manifest(json!({"requests": ["list_issues"]})), 0);
    source.answer(json!({"issues": [listed("group/app#14", "Fix the login"), listed("group/app!31", "Review the fix")]}));

    ok(dir.path(), &["inbox", "sync"]);

    let keys: Vec<String> = inbox(dir.path()).iter().map(|issue| issue["key"].as_str().unwrap().to_string()).collect();
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(keys.contains(&"group/app#14".to_string()) && keys.contains(&"group/app!31".to_string()));
    let shown = document(dir.path(), &["inbox", "show", "group/app!31", "--json"]);
    assert_eq!(shown["issue"]["summary"], json!("Review the fix"));
    assert_eq!(shown["issue"]["source"], json!("forge"));
}

#[test]
#[serial]
fn a_listing_with_a_key_no_command_line_can_carry_is_refused_whole() {
    let dir = sandbox();
    let source = Fixture::install("forge", manifest(json!({"requests": ["list_issues"]})), 0);
    source.answer(json!({"issues": [listed("group/app#14", "Fine"), listed("two words", "Not a key")]}));

    let said = text(&kasl(dir.path(), &["inbox", "sync"]));

    assert!(said.contains("a key kasl cannot keep"), "{said}");
    assert!(inbox(dir.path()).is_empty(), "half a listing was kept");
}

#[test]
#[serial]
fn find_tasks_hands_the_plugin_the_day_and_reads_what_it_found() {
    let dir = sandbox();
    // Discovery here runs in this process and searches its PATH: keep it to
    // the system's tools, so no plugin the machine really has is asked.
    let path = std::env::var_os("PATH");
    // SAFETY: every test here is #[serial].
    unsafe { std::env::set_var("PATH", system_path()) };
    a_working_day();
    let finder = Fixture::install("finder", manifest(json!({"requests": ["find_tasks"]})), 0);
    finder.answer(json!({"tasks": [{"name": "Fix the login", "ref": "a1b2c3d"}, {"name": "Write it down"}], "message": "1 push could not be read"}));

    let finders = kasl::libs::plugin::tasks::finders().unwrap();
    let names: Vec<&str> = finders.iter().map(|plugin| plugin.name.as_str()).collect();
    assert_eq!(names, ["finder"]);
    let today = Local::now().date_naive();
    let day = serde_json::to_value(kasl::libs::report::DayReport::read(today).unwrap().to_json()).unwrap();
    let found = kasl::libs::plugin::tasks::find(&finders[0], day).unwrap();
    if let Some(path) = path {
        // SAFETY: as above.
        unsafe { std::env::set_var("PATH", path) };
    }

    let found_names: Vec<&str> = found.tasks.iter().map(|task| task.name.as_str()).collect();
    assert_eq!(found_names, ["Fix the login", "Write it down"]);
    assert_eq!(found.tasks[0].reference.as_deref(), Some("a1b2c3d"));
    assert_eq!(found.message.as_deref(), Some("1 push could not be read"));
    let call = &finder.calls()[0];
    assert_eq!(call["request"], json!("find_tasks"));
    assert_eq!(call["subject"]["date"], json!(today.format("%Y-%m-%d").to_string()), "the day is handed whole");
    assert_eq!(call["subject"]["tasks"][0]["name"], json!("Write the plugin protocol"));

    let said = ok(dir.path(), &["plugin", "list"]);
    assert!(said.contains("offers tasks to `kasl task find`"), "{said}");
    assert!(!said.contains("a source"), "finding tasks does not make it an inbox source: {said}");
}

#[test]
#[serial]
fn a_comment_goes_to_the_issues_source_and_says_which_one_it_was() {
    // Found on the v1.20 live run: the comment reached the plugin, and kasl
    // then reported it as left "in Jira".
    let dir = sandbox();
    let source = Fixture::install("forge", manifest(json!({"requests": ["list_issues", "comment"]})), 0);
    source.answer(json!({"issues": [listed("group/app!31", "Review the fix")]}));
    ok(dir.path(), &["inbox", "sync"]);
    source.answer(json!({}));

    let said = ok(dir.path(), &["inbox", "comment", "group/app!31", "Looks good"]);

    assert!(said.contains("Commented on group/app!31 in forge"), "{said}");
    assert!(!said.contains("Jira"), "{said}");
    let call = source.calls().into_iter().find(|call| call["request"] == json!("comment")).unwrap();
    assert_eq!(call["comment"], json!("Looks good"));
    assert_eq!(call["subject"]["key"], json!("group/app!31"));
}
