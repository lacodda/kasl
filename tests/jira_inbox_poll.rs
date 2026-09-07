//! The inbox poll against a Jira that does not answer.
//!
//! A failed poll used to come back as an empty issue list, which the sync
//! then reconciled: every issue got `gone_at`, and the next successful poll
//! brought all of them "back" - one change toast per issue, every morning
//! after the VPN dropped overnight. A failed poll is an error, not an inbox.

use kasl::api::Session;
use kasl::api::jira::{Jira, JiraConfig};
use kasl::db::jira_inbox::{JiraInbox, JiraInboxUpsert};
use kasl::libs::config::{Config, JiraInboxConfig};
use kasl::libs::data_storage::DataStorage;
use kasl::libs::jira_inbox::{TOAST_STORM_THRESHOLD, sync_noninteractive, toasts_collapse};
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A throwaway data directory, so the test never reads or writes the
/// machine's own config, database, or cached session.
fn sandbox() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    // SAFETY: tests touching the env are #[serial]
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("LOCALAPPDATA", dir.path());
    }
    dir
}

/// Points the config at the mock server, with toasts off: the sync under
/// test must not pop real notifications on the machine running the tests.
fn configure(server: &MockServer) -> JiraInboxConfig {
    let inbox = JiraInboxConfig {
        notify: false,
        notify_changes: false,
        notify_gone: false,
        ..Default::default()
    };
    Config {
        jira: Some(JiraConfig {
            login: "agent".to_string(),
            api_url: server.uri(),
            completed_statuses: Vec::new(),
        }),
        jira_inbox: Some(inbox.clone()),
        ..Default::default()
    }
    .save()
    .unwrap();

    // A cached session, so the poll goes straight to the search endpoint.
    let session = DataStorage::new().get_path(".jira_session_id").unwrap();
    Jira::write_session_id(session.to_str().unwrap(), "JSESSIONID=cafe").unwrap();
    inbox
}

fn seed(key: &str) -> JiraInboxUpsert {
    JiraInboxUpsert {
        issue_key: key.to_string(),
        issue_id: "1".to_string(),
        summary: format!("Summary for {key}"),
        status_id: Some("10".to_string()),
        status_name: "Open".to_string(),
        priority: Some("Medium".to_string()),
        priority_rank: 3,
        sort_value: Some(5.0),
        url: format!("https://jira.example.com/browse/{key}"),
        raw_updated: None,
    }
}

fn issue_json(key: &str) -> serde_json::Value {
    json!({
        "id": "1",
        "key": key,
        "fields": {
            "summary": format!("Summary for {key}"),
            "status": {"id": "10", "name": "Open"},
            "priority": {"name": "Medium", "id": "3"},
            "updated": "2026-09-07T10:00:00.000+0000"
        }
    })
}

#[tokio::test]
#[serial]
async fn a_poll_that_fails_leaves_the_inbox_untouched() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server);
    JiraInbox::new().unwrap().upsert_batch(&[seed("KA-1"), seed("KA-2")]).unwrap();

    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let result = sync_noninteractive(&inbox).await;
    let err = result.expect_err("a 503 from Jira must fail the poll, not answer with an empty inbox");
    assert!(err.to_string().contains("503"), "the error names the status: {err}");

    let items = JiraInbox::new().unwrap().list_active(true).unwrap();
    assert_eq!(items.len(), 2);
    assert!(
        items.iter().all(|i| i.gone_at.is_none()),
        "a failed poll marked issues gone: {:?}",
        items.iter().filter(|i| i.gone_at.is_some()).map(|i| &i.issue_key).collect::<Vec<_>>()
    );
}

#[tokio::test]
#[serial]
async fn a_poll_that_answers_still_reconciles() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server);
    JiraInbox::new().unwrap().upsert_batch(&[seed("KA-1"), seed("KA-2")]).unwrap();

    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "startAt": 0, "maxResults": 100, "total": 1,
            "issues": [issue_json("KA-1")]
        })))
        .mount(&server)
        .await;

    let outcome = sync_noninteractive(&inbox).await.unwrap();
    assert!(!outcome.skipped);
    assert_eq!(outcome.fetched, 1);
    assert_eq!(outcome.gone_keys, vec!["KA-2".to_string()]);

    let items = JiraInbox::new().unwrap().list_active(true).unwrap();
    let gone: Vec<&str> = items.iter().filter(|i| i.gone_at.is_some()).map(|i| i.issue_key.as_str()).collect();
    assert_eq!(gone, vec!["KA-2"]);
}

#[test]
fn a_handful_of_toasts_show_one_by_one_and_a_storm_becomes_one() {
    assert!(!toasts_collapse(0));
    assert!(!toasts_collapse(TOAST_STORM_THRESHOLD));
    assert!(toasts_collapse(TOAST_STORM_THRESHOLD + 1));
    assert!(toasts_collapse(200));
}
