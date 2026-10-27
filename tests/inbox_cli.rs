//! `kasl inbox` with filters, run as the binary against a seeded sandbox.
//!
//! The filter logic has its own tests; this holds the seam the user sees -
//! that the flags reach the filter, that a cut list announces itself as a
//! slice of the whole, and that a bad window or an unknown priority is an
//! error and not an empty list.

use chrono::{Duration, Local};
use kasl::db::inbox::{Inbox, InboxUpsert};
use serial_test::serial;
use std::path::Path;
use std::process::{Command, Stdio};
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

fn kasl(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_kasl"))
        .args(args)
        .env("HOME", dir)
        .env("LOCALAPPDATA", dir)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .expect("kasl runs");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn seed(key: &str, priority: &str, rank: i32, score: f64) -> InboxUpsert {
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
        issue_type: None,
        project: None,
    }
}

/// Three issues: two fresh, one a month old and pushed back in time by hand.
fn seed_inbox() {
    let db = Inbox::new().unwrap();
    db.upsert_batch(&[seed("KA-1", "High", 2, 8.0), seed("KA-2", "Medium", 3, 3.0), seed("KA-3", "Low", 4, 9.0)])
        .unwrap();
    let month_ago = Local::now().naive_local() - Duration::days(30);
    drop(db);
    kasl::db::db::Db::new()
        .unwrap()
        .conn
        .execute("UPDATE inbox SET first_seen = ?1 WHERE issue_key = 'KA-3'", rusqlite::params![month_ago])
        .unwrap();
}

#[test]
#[serial]
fn a_cut_list_says_how_much_of_the_whole_it_is() {
    let dir = sandbox();
    seed_inbox();

    let (ok, out) = kasl(dir.path(), &["inbox", "--since", "7d"]);
    assert!(ok, "{out}");
    assert!(out.contains("2 of 3 issues (since 7d)"), "the header does not announce the slice:\n{out}");
    assert!(out.contains("KA-1") && out.contains("KA-2") && !out.contains("KA-3"), "{out}");

    let (ok, out) = kasl(dir.path(), &["inbox", "--priority", "High+", "--min-score", "5"]);
    assert!(ok, "{out}");
    assert!(out.contains("1 of 3 issues (score ≥ 5, priority High+)"), "{out}");
    assert!(out.contains("KA-1") && !out.contains("KA-3"), "{out}");

    let (ok, out) = kasl(dir.path(), &["inbox"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("Jira inbox:") && !out.contains(" of 3 "),
        "the whole inbox must not read as a slice:\n{out}"
    );
}

#[test]
#[serial]
fn the_same_cuts_reach_list_and_the_pickers() {
    let dir = sandbox();
    seed_inbox();

    let (ok, out) = kasl(dir.path(), &["inbox", "list", "--sort", "priority", "-n", "1"]);
    assert!(ok, "{out}");
    assert!(out.contains("1 of 3 issues (by priority)") && out.contains("KA-1"), "{out}");

    // No terminal, so the picker cannot open - but the filter is applied
    // first, and an empty slice is named as such rather than as an empty inbox.
    let (ok, out) = kasl(dir.path(), &["inbox", "take", "--since", "1h", "--min-score", "100"]);
    assert!(!ok);
    assert!(out.contains("no issue matches the filter") && out.contains("3 in the inbox"), "{out}");
}

#[test]
#[serial]
fn nothing_matching_is_not_an_empty_inbox() {
    let dir = sandbox();
    seed_inbox();
    let (ok, out) = kasl(dir.path(), &["inbox", "--status", "Done"]);
    assert!(ok, "{out}");
    assert!(out.contains("No issue matches (status Done); 3 in the inbox"), "{out}");
    assert!(!out.contains("Jira inbox is empty"), "{out}");
}

#[test]
#[serial]
fn a_bad_window_or_an_unknown_priority_is_an_error() {
    let dir = sandbox();
    seed_inbox();

    let (ok, out) = kasl(dir.path(), &["inbox", "--since", "week"]);
    assert!(!ok, "{out}");
    assert!(out.contains("'week' is not a window"), "{out}");

    let (ok, out) = kasl(dir.path(), &["inbox", "--priority", "Urgent"]);
    assert!(!ok, "{out}");
    assert!(
        out.contains("no priority named 'Urgent'") && out.contains("High") && out.contains("Low"),
        "{out}"
    );
}

#[test]
#[serial]
fn writing_to_jira_without_jira_says_what_is_missing() {
    let dir = sandbox();
    seed_inbox();

    for args in [
        &["inbox", "close", "KA-1"][..],
        &["inbox", "start", "KA-1"],
        &["inbox", "comment", "KA-1", "done"],
    ] {
        let (ok, out) = kasl(dir.path(), args);
        assert!(!ok, "{args:?} cannot succeed without Jira:\n{out}");
        assert!(out.contains("Jira is not configured"), "{args:?} must say why:\n{out}");
    }
}

#[test]
#[serial]
fn a_comment_needs_its_text_outside_a_terminal() {
    let dir = sandbox();
    seed_inbox();

    let (ok, out) = kasl(dir.path(), &["inbox", "comment", "KA-1"]);
    assert!(!ok, "{out}");
    assert!(out.contains("comment text is required"), "{out}");
}

/// The whole loop from the far side: Jira resolves a taken issue, and a sync
/// run from a script says so and names where the task can be finished.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn a_sync_off_a_terminal_reports_a_resolved_issue_with_an_open_task() {
    use kasl::api::Session;
    use kasl::api::jira::{Jira, JiraConfig};
    use kasl::libs::config::{Config, InboxConfig};
    use kasl::libs::jira_bridge::MoveOptions;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let dir = sandbox();
    let server = MockServer::start().await;
    Config {
        jira: Some(JiraConfig {
            login: "agent".to_string(),
            api_url: server.uri(),
            completed_statuses: Vec::new(),
        }),
        inbox: Some(InboxConfig {
            notify: false,
            ..Default::default()
        }),
        ..Default::default()
    }
    .save()
    .unwrap();
    let session = kasl::libs::data_storage::DataStorage::new().get_path(".jira_session_id").unwrap();
    Jira::write_session_id(session.to_str().unwrap(), "JSESSIONID=cafe").unwrap();

    seed_inbox();
    kasl::libs::toast_apply::take("KA-1", &MoveOptions::unattended()).await.unwrap();

    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"startAt": 0, "maxResults": 100, "total": 0, "issues": []})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/api/2/issue/KA-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "key": "KA-1",
            "fields": {"status": {"id": "5", "name": "Done", "statusCategory": {"key": "done"}}, "resolution": {"id": "1", "name": "Done"}}
        })))
        .mount(&server)
        .await;

    let (ok, out) = kasl(dir.path(), &["inbox", "sync"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("KA-1 was resolved in Jira (Done); its task is still open") && out.contains("kasl task find"),
        "the sync must say what Jira did and where to finish the task:\n{out}"
    );

    let (ok, out) = kasl(dir.path(), &["inbox", "list", "--all"]);
    assert!(ok, "{out}");
    assert!(out.contains("resolved"), "the list shows why KA-1 left:\n{out}");
}
