//! The MCP server as a client meets it: a real process, real JSON-RPC lines,
//! against a seeded sandbox.
//!
//! The unit tests beside the code check the shapes; these check the wire and
//! the promise. A server that builds a correct tool list and then writes
//! something else to stdout - a migration notice, a stray print, an answer to
//! a notification - is broken in exactly the way a client cannot recover
//! from, and only a process test sees it. The promise is that every tool
//! answers with the document its `--json` command prints, so the documents
//! are compared whole, field for field, with what the command prints.

use chrono::{Duration, Local, NaiveDate};
use kasl::db::inbox::{Inbox, InboxUpsert};
use serde_json::{Value, json};
use serial_test::serial;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};
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

/// The binary in the sandbox, with no debug setting inherited from whoever runs the tests.
fn kasl(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kasl"));
    command.env("HOME", dir).env("LOCALAPPDATA", dir);
    for name in ["KASL_DEBUG", "RUST_LOG", "NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE"] {
        command.env_remove(name);
    }
    command
}

fn run(dir: &Path, args: &[&str]) -> Output {
    kasl(dir).args(args).stdin(Stdio::null()).output().expect("kasl runs")
}

/// What a `--json` command prints, as a value.
fn document(dir: &Path, args: &[&str]) -> Value {
    let out = run(dir, args);
    assert!(out.status.success(), "kasl {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("kasl {args:?}: not one JSON document ({e})"))
}

/// A running server, with the pipes a client talks through.
struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start(dir: &Path) -> Server {
        let mut child = kasl(dir)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Not piped: nobody reads it here, and a full pipe would stall the server.
            .stderr(Stdio::null())
            .spawn()
            .expect("kasl mcp starts");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server { child, stdin, stdout }
    }

    fn send_line(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Sends a request and reads the one line that answers it.
    fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send_line(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string());
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("not JSON on stdout: {line:?} ({e})"));
        assert_eq!(response["jsonrpc"], "2.0", "{response}");
        assert_eq!(response["id"], id, "an answer must carry the id it answers: {response}");
        response
    }

    /// Calls a tool that must succeed, and returns its document.
    ///
    /// The text block must be the same document as the structured one: a
    /// client that reads only text gets what a client reading the structure gets.
    fn call(&mut self, id: i64, name: &str, arguments: Value) -> Value {
        let response = self.request(id, "tools/call", json!({ "name": name, "arguments": arguments }));
        let result = &response["result"];
        assert!(result["isError"] != json!(true), "{name} failed: {result}");
        let structured = result["structuredContent"].clone();
        assert!(structured.is_object(), "{name}: the document is an object: {result}");
        let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, structured, "{name}: the text and the structure disagree");
        structured
    }

    /// Calls a tool that must fail, and returns what it told the model.
    fn call_error(&mut self, id: i64, name: &str, arguments: Value) -> String {
        let response = self.request(id, "tools/call", json!({ "name": name, "arguments": arguments }));
        assert!(response["error"].is_null(), "a failing tool answers the model, not the client: {response}");
        assert_eq!(response["result"]["isError"], true, "{name} should have failed: {response}");
        response["result"]["content"][0]["text"].as_str().unwrap().to_string()
    }

    /// Closes stdin and waits: the server must end when the client goes away,
    /// having written nothing after its last answer.
    fn finish(mut self) {
        drop(self.stdin);
        let mut rest = String::new();
        self.stdout.read_to_string(&mut rest).unwrap();
        assert!(rest.is_empty(), "stdout carried more than answers: {rest:?}");
        let status = self.child.wait().unwrap();
        assert!(status.success(), "the server exited with {status}");
    }
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

fn yesterday() -> NaiveDate {
    Local::now().date_naive() - Duration::days(1)
}

/// Yesterday, closed: 09:00 to 17:00 with a half-hour lunch and, just after
/// it, a second pause that leaves a five-minute stretch between the two - a
/// short interval the report drops. Two tasks written that day, a tag, and an
/// inbox with a pinned, a sleeping and a gone issue.
fn seed(dir: &Path) {
    for args in [
        &["task", "add", "--name", "Wire the MCP server", "--tags", "cli,mcp"][..],
        &["task", "add", "--name", "Second task", "--completeness", "40"][..],
    ] {
        let out = run(dir, args);
        assert!(out.status.success(), "seeding {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    let day = yesterday().format("%Y-%m-%d").to_string();
    let db = kasl::db::db::Db::new().unwrap();
    db.conn
        .execute(
            "INSERT INTO workdays (date, start, end) VALUES (?1, ?2, ?3)",
            rusqlite::params![day, format!("{day} 09:00:00"), format!("{day} 17:00:00")],
        )
        .unwrap();
    for (start, end, seconds) in [("12:00:00", "12:30:00", 1800), ("12:35:00", "13:00:00", 1500)] {
        db.conn
            .execute(
                "INSERT INTO pauses (start, end, duration, protected) VALUES (?1, ?2, ?3, 1)",
                rusqlite::params![format!("{day} {start}"), format!("{day} {end}"), seconds],
            )
            .unwrap();
    }
    db.conn
        .execute("UPDATE tasks SET timestamp = ?1", rusqlite::params![format!("{day} 10:00:00")])
        .unwrap();

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
fn a_session_lists_the_tools_and_stdout_carries_nothing_but_answers() {
    let dir = sandbox();
    let mut server = Server::start(dir.path());

    let init = server.request(
        1,
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "kasl");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    // Neither a notification nor a line that is not JSON gets an answer: the
    // next line on stdout must be the answer to the ping that follows them.
    server.send_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    server.send_line("this is not JSON");
    server.send_line("");
    assert_eq!(server.request(2, "ping", json!({}))["result"], json!({}));

    let tools = server.request(3, "tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["day", "pauses", "week", "month", "tasks", "inbox", "issue", "server_status", "server_queue"]
    );

    // A first database is made on the first read; the migrations' notices
    // must not reach stdout, where they would be read as protocol.
    let day = server.call(4, "day", json!({}));
    assert_eq!(day["date"], Local::now().date_naive().format("%Y-%m-%d").to_string());
    assert!(day["workday"].is_null());

    let unknown = server.request(5, "resources/list", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);

    server.finish();
}

#[test]
#[serial]
fn every_tool_answers_with_the_document_its_command_prints() {
    let dir = sandbox();
    seed(dir.path());
    let day = yesterday().format("%Y-%m-%d").to_string();
    let mut server = Server::start(dir.path());

    let pairs: Vec<(&str, Value, Vec<&str>)> = vec![
        ("day", json!({ "date": day }), vec!["report", "--last", "--json"]),
        ("pauses", json!({ "date": day }), vec!["pauses", "list", "--date", &day, "--json"]),
        (
            "pauses",
            json!({ "date": day, "min_minutes": 26 }),
            vec!["pauses", "list", "--date", &day, "--min-duration", "26", "--json"],
        ),
        ("month", json!({}), vec!["sum", "--json"]),
        ("tasks", json!({ "date": day }), vec!["task", "list", "--all", "--json"]),
        ("tasks", json!({ "all": true }), vec!["task", "list", "--all", "--json"]),
        ("tasks", json!({ "tag": "mcp" }), vec!["task", "list", "--tag", "mcp", "--json"]),
        ("tasks", json!({ "ids": [2] }), vec!["task", "show", "2", "--json"]),
        ("inbox", json!({}), vec!["inbox", "list", "--json"]),
        (
            "inbox",
            json!({ "all": true, "snoozed": true }),
            vec!["inbox", "list", "--all", "--snoozed", "--json"],
        ),
        (
            "inbox",
            json!({ "priority": "High+", "sort": "priority", "limit": 1 }),
            vec!["inbox", "list", "--priority", "High+", "--sort", "priority", "-n", "1", "--json"],
        ),
        (
            "inbox",
            json!({ "status": "Open", "project": ["KA"] }),
            vec!["inbox", "list", "--status", "Open", "--project", "KA", "--json"],
        ),
        ("issue", json!({ "key": "KA-1", "why": true }), vec!["inbox", "show", "KA-1", "--why", "--json"]),
        ("server_status", json!({}), vec!["server", "status", "--json"]),
        ("server_queue", json!({}), vec!["server", "queue", "--json"]),
    ];
    for (id, (tool, arguments, command)) in pairs.into_iter().enumerate() {
        let served = server.call(id as i64 + 1, tool, arguments.clone());
        let printed = document(dir.path(), &command);
        assert_eq!(served, printed, "{tool} {arguments} differs from kasl {command:?}");
    }
    server.finish();
}

#[test]
#[serial]
fn a_day_reads_the_same_in_the_report_the_week_and_the_month() {
    // The week and the month count a day as the report does: sliced at its
    // pauses, the five-minute stretch between them dropped. Counted as gross
    // time minus pauses, the month read 300 seconds more than the report.
    let dir = sandbox();
    seed(dir.path());
    let day = yesterday().format("%Y-%m-%d").to_string();
    let mut server = Server::start(dir.path());

    let report = server.call(1, "day", json!({ "date": day }));
    assert_eq!(report["dropped"]["count"], 1, "{report}");
    // 8 h, less the two pauses (55 min) and the 5-minute stretch between them.
    assert_eq!(report["work_seconds"], 8 * 3600 - 55 * 60 - 5 * 60, "{report}");

    let week = server.call(2, "week", json!({ "date": day }));
    let month = server.call(3, "month", json!({ "month": &day[..7] }));
    for summary in [&week, &month] {
        let days = summary["days"].as_array().unwrap();
        let that_day = days.iter().find(|d| d["date"] == day).unwrap_or_else(|| panic!("{day} missing from {summary}"));
        assert_eq!(that_day["work_seconds"], report["work_seconds"], "{summary}");
        assert_eq!(that_day["productivity"], report["productivity"], "{summary}");
        let total: i64 = days.iter().map(|d| d["work_seconds"].as_i64().unwrap()).sum();
        assert_eq!(summary["total_seconds"], total, "{summary}");
    }

    // The week runs Monday to Sunday around the day asked about.
    let from = NaiveDate::parse_from_str(week["from"].as_str().unwrap(), "%Y-%m-%d").unwrap();
    let to = NaiveDate::parse_from_str(week["to"].as_str().unwrap(), "%Y-%m-%d").unwrap();
    assert_eq!(from.format("%A").to_string(), "Monday");
    assert_eq!(to - from, Duration::days(6));
    assert!(from <= yesterday() && yesterday() <= to);

    // A period asked for by name is that period, not the current one.
    let long_ago = server.call(4, "month", json!({ "month": "2001-02" }));
    assert_eq!(long_ago["month"], "2001-02");
    assert_eq!(long_ago["days"], json!([]));
    let long_ago = server.call(5, "week", json!({ "date": "2001-02-14" }));
    assert_eq!(
        (&long_ago["week"], &long_ago["from"], &long_ago["to"]),
        (&json!("2001-W07"), &json!("2001-02-12"), &json!("2001-02-18"))
    );
    server.finish();
}

#[test]
#[serial]
fn an_unclosed_day_ends_at_the_last_activity_any_pause_saw() {
    // Yesterday was never closed: the watcher saw a lunch, then work until
    // half past six between two short breaks. The day ends where the last
    // pause of any length ended - 18:32 - not where the last long one did:
    // ended at the lunch, the report used to lose the whole afternoon, and
    // the month that counted it had a different day from the report.
    let dir = sandbox();
    let day = yesterday().format("%Y-%m-%d").to_string();
    let db = kasl::db::db::Db::new().unwrap();
    db.conn
        .execute(
            "INSERT INTO workdays (date, start) VALUES (?1, ?2)",
            rusqlite::params![day, format!("{day} 09:00:00")],
        )
        .unwrap();
    for (start, end, seconds) in [("12:00:00", "13:00:00", 3600), ("18:00:00", "18:05:00", 300), ("18:30:00", "18:32:00", 120)] {
        db.conn
            .execute(
                "INSERT INTO pauses (start, end, duration) VALUES (?1, ?2, ?3)",
                rusqlite::params![format!("{day} {start}"), format!("{day} {end}"), seconds],
            )
            .unwrap();
    }

    let report = document(dir.path(), &["report", "--last", "--json"]);
    let intervals = report["intervals"].as_array().unwrap();
    assert_eq!(intervals.last().unwrap()["end"], format!("{day}T18:32:00"), "{report}");
    // 09:00-12:00 and 13:00-18:32; the short breaks lower productivity, not the time.
    assert_eq!(report["work_seconds"], 3 * 3600 + 5 * 3600 + 32 * 60, "{report}");
    // (30720 - 420) / 30720, measured to the same end.
    assert_eq!(report["productivity"], 98.6, "{report}");

    let mut server = Server::start(dir.path());
    let week = server.call(1, "week", json!({ "date": day }));
    let month = server.call(2, "month", json!({ "month": &day[..7] }));
    for summary in [&week, &month] {
        let that_day = summary["days"].as_array().unwrap().iter().find(|d| d["date"] == day).unwrap();
        assert_eq!(that_day["work_seconds"], report["work_seconds"], "{summary}");
    }
    server.finish();
}

#[test]
#[serial]
fn a_failing_read_answers_the_model_and_the_session_goes_on() {
    let dir = sandbox();
    seed(dir.path());
    let mut server = Server::start(dir.path());

    let missing = server.call_error(1, "issue", json!({ "key": "KA-99" }));
    assert!(missing.contains("KA-99"), "{missing}");
    let missing = server.call_error(2, "tasks", json!({ "ids": [404] }));
    assert!(missing.contains("404"), "{missing}");
    let misspelt = server.call_error(3, "day", json!({ "data": "2026-10-19" }));
    assert!(misspelt.contains("`data`"), "{misspelt}");

    // Still serving after three failures.
    let inbox = server.call(4, "inbox", json!({}));
    assert!(inbox["issues"].as_array().is_some_and(|issues| !issues.is_empty()), "{inbox}");
    server.finish();
}

#[test]
#[serial]
fn a_moment_with_a_fraction_of_a_second_is_read_not_a_crash() {
    // kasl writes whole seconds, but a row edited by hand or by another tool
    // may not; parsing one used to panic `kasl report`, and under MCP a panic
    // would take every later call down with it.
    let dir = sandbox();
    let day = yesterday().format("%Y-%m-%d").to_string();
    let db = kasl::db::db::Db::new().unwrap();
    db.conn
        .execute(
            "INSERT INTO workdays (date, start, end) VALUES (?1, ?2, ?3)",
            rusqlite::params![day, format!("{day} 09:00:00.250"), format!("{day}T17:00:00")],
        )
        .unwrap();

    let printed = document(dir.path(), &["report", "--last", "--json"]);
    assert_eq!(printed["workday"]["start"], format!("{day}T09:00:00"));

    let mut server = Server::start(dir.path());
    let served = server.call(1, "day", json!({ "date": day }));
    assert_eq!(served["workday"], printed["workday"]);
    server.finish();
}

#[test]
#[serial]
fn a_client_that_hangs_up_before_its_answer_ends_the_session_quietly() {
    // The client is gone: the answer cannot be delivered, and there is nobody
    // left to report a failure to. A closed pipe is the end of the session,
    // not an error exit.
    let dir = sandbox();
    let mut server = Server::start(dir.path());
    let Server { mut child, mut stdin, stdout } = {
        server.request(1, "ping", json!({}));
        server
    };
    drop(stdout);
    // The tool list is long enough not to fit in what a closed pipe still accepts.
    for id in 2..20 {
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list" }).to_string();
        if writeln!(stdin, "{line}").and_then(|()| stdin.flush()).is_err() {
            break;
        }
    }
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success(), "a client hanging up is not a failure: {status}");
}

#[test]
#[serial]
fn a_client_that_asks_nothing_gets_nothing_on_stdout() {
    // The server speaks only when spoken to: no banner, no greeting, a clean
    // exit when the stream closes. The hint for a person who typed it goes to
    // stderr, and only when stdin is a terminal.
    let dir = sandbox();
    let out = kasl(dir.path()).arg("mcp").stdin(Stdio::null()).output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "{:?}", String::from_utf8_lossy(&out.stdout));
}
