//! The inbox poll against a Jira that does not answer.
//!
//! A failed poll used to come back as an empty issue list, which the sync
//! then reconciled: every issue got `gone_at`, and the next successful poll
//! brought all of them "back" - one change toast per issue, every morning
//! after the VPN dropped overnight. A failed poll is an error, not an inbox.
//!
//! The same storm had more ways in: an expired session answered as the
//! anonymous user, a list that shifted while its pages were read, and a
//! trickle of changes small enough per poll to slip under a per-poll limit.
//! Each has its test here.

use chrono::{Duration, NaiveDate, NaiveDateTime};
use kasl::api::Session;
use kasl::api::jira::{Jira, JiraConfig};
use kasl::db::jira_inbox::{JiraInbox, JiraInboxUpsert};
use kasl::libs::config::{Config, JiraInboxConfig};
use kasl::libs::data_storage::DataStorage;
use kasl::libs::jira_inbox::{TOAST_STORM_THRESHOLD, TOAST_WINDOW_MINUTES, ToastBudget, ToastPlan, sync_noninteractive, toasts_collapse};
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;
use wiremock::matchers::{method, path, query_param};
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

    let result = sync_noninteractive(&inbox, &mut ToastBudget::new()).await;
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

    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
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

/// Jira Server answers an expired session as the anonymous user, with 200
/// and an empty page. That answer is a lost session, not an empty inbox.
#[tokio::test]
#[serial]
async fn an_answer_given_to_nobody_leaves_the_inbox_untouched() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server);
    JiraInbox::new().unwrap().upsert_batch(&[seed("KA-1"), seed("KA-2")]).unwrap();

    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-AUSERNAME", "anonymous")
                .insert_header("X-Seraph-LoginReason", "AUTHENTICATED_FAILED")
                .set_body_json(json!({"startAt": 0, "maxResults": 100, "total": 0, "issues": []})),
        )
        .mount(&server)
        .await;

    // With the session refused there is no way back in without a stored
    // secret, so the poll either fails or is skipped - both are fine; what
    // must not happen is a reconcile against the empty page.
    if let Ok(outcome) = sync_noninteractive(&inbox, &mut ToastBudget::new()).await {
        assert!(outcome.skipped, "an anonymous answer was taken as the inbox: {outcome:?}");
    }

    let items = JiraInbox::new().unwrap().list_active(true).unwrap();
    assert!(
        items.iter().all(|i| i.gone_at.is_none()),
        "an anonymous answer marked issues gone: {:?}",
        items.iter().filter(|i| i.gone_at.is_some()).map(|i| &i.issue_key).collect::<Vec<_>>()
    );
}

/// An issue resolved while the pages are read shifts the rest by one, and a
/// still-open issue falls between two pages. Such a list is short, and it
/// must not mark the missing issue gone.
#[tokio::test]
#[serial]
async fn a_list_that_shifted_between_pages_leaves_the_inbox_untouched() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server);
    JiraInbox::new().unwrap().upsert_batch(&[seed("KA-1"), seed("KA-2"), seed("KA-3")]).unwrap();

    // Page one holds KA-1 and KA-2 of three. By the time page two is read,
    // KA-1 is resolved and KA-3 has moved left into the offset already read,
    // so page two is empty and KA-3 is never read at all.
    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .and(query_param("startAt", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "startAt": 0, "maxResults": 2, "total": 3,
            "issues": [issue_json("KA-1"), issue_json("KA-2")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .and(query_param("startAt", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "startAt": 2, "maxResults": 2, "total": 2,
            "issues": []
        })))
        .mount(&server)
        .await;

    let result = sync_noninteractive(&inbox, &mut ToastBudget::new()).await;
    let err = result.expect_err("a list short of what Jira counted must fail the poll");
    assert!(err.to_string().contains("2 of 3"), "the error says what was short: {err}");

    let items = JiraInbox::new().unwrap().list_active(true).unwrap();
    assert!(items.iter().all(|i| i.gone_at.is_none()), "a short list marked issues gone");
}

/// An issue read twice across pages is one issue, not a failed poll.
#[tokio::test]
#[serial]
async fn an_issue_read_twice_is_still_one_issue() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server);

    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .and(query_param("startAt", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "startAt": 0, "maxResults": 2, "total": 3,
            "issues": [issue_json("KA-1"), issue_json("KA-2")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .and(query_param("startAt", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "startAt": 2, "maxResults": 2, "total": 3,
            "issues": [issue_json("KA-2"), issue_json("KA-3")]
        })))
        .mount(&server)
        .await;

    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert_eq!(outcome.fetched, 3);
    assert_eq!(outcome.new_keys, vec!["KA-1", "KA-2", "KA-3"]);
}

#[test]
#[serial]
fn only_a_status_or_priority_change_is_worth_a_toast() {
    let _dir = sandbox();
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[seed("KA-1"), seed("KA-2"), seed("KA-3"), seed("KA-4")]).unwrap();

    let mut rescored = seed("KA-1");
    rescored.sort_value = Some(8.0);
    let mut moved = seed("KA-2");
    moved.status_id = Some("11".to_string());
    moved.status_name = "In Progress".to_string();
    let mut raised = seed("KA-3");
    raised.priority = Some("High".to_string());
    raised.priority_rank = 2;
    let result = db.upsert_batch(&[rescored, moved, raised, seed("KA-4")]).unwrap();
    let notable = |key: &str| result.changed.iter().find(|c| c.issue_key == key).map(|c| c.notable);
    assert_eq!(notable("KA-1"), Some(false), "a score recomputed by Jira is not news");
    assert_eq!(notable("KA-2"), Some(true), "a status change is");
    assert_eq!(notable("KA-3"), Some(true), "a priority change is");
    assert_eq!(notable("KA-4"), None, "nothing changed, nothing to say");

    // Missed by one poll and read again by the next: the inbox corrects its
    // own view, which is not something that happened to the issue.
    db.mark_gone(&["KA-1".to_string(), "KA-2".to_string(), "KA-3".to_string()]).unwrap();
    let result = db.upsert_batch(&[seed("KA-4")]).unwrap();
    let back = result.changed.iter().find(|c| c.issue_key == "KA-4").expect("the return is recorded");
    assert!(back.change.contains("back"), "the badge still says so: {}", back.change);
    assert!(!back.notable, "a return is not worth a toast");
}

fn at(minutes: i64) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 9, 23).unwrap().and_hms_opt(9, 0, 0).unwrap() + Duration::minutes(minutes)
}

/// The bug this budget exists for: three changes on every five-minute poll
/// stay under any per-poll threshold, and used to toast every one of them.
#[test]
fn a_trickle_across_polls_is_bounded_like_a_storm() {
    let mut budget = ToastBudget::new();
    let mut single = 0;
    let mut summaries = 0;
    for poll in 0..12 {
        match budget.admit(at(poll * 5), 3) {
            ToastPlan::Each => single += 3,
            ToastPlan::Summary => summaries += 1,
            ToastPlan::Quiet => {}
        }
    }
    assert!(single <= TOAST_STORM_THRESHOLD, "{single} single toasts in an hour");
    assert_eq!(summaries, 1, "one summary says the rest is in the list");
}

#[test]
fn a_storm_in_one_poll_becomes_one_summary() {
    let mut budget = ToastBudget::new();
    assert_eq!(budget.admit(at(0), 200), ToastPlan::Summary);
    assert_eq!(budget.admit(at(5), 200), ToastPlan::Quiet, "the hour already had its summary");
}

#[test]
fn a_quiet_hour_gives_the_budget_back() {
    let mut budget = ToastBudget::new();
    assert_eq!(budget.admit(at(0), TOAST_STORM_THRESHOLD), ToastPlan::Each);
    assert_eq!(budget.admit(at(1), 1), ToastPlan::Summary);
    assert_eq!(budget.admit(at(2), 1), ToastPlan::Quiet);
    assert_eq!(
        budget.admit(at(TOAST_WINDOW_MINUTES + 1), 1),
        ToastPlan::Each,
        "an hour later one issue gets its toast again"
    );
    assert_eq!(budget.admit(at(TOAST_WINDOW_MINUTES + 2), TOAST_STORM_THRESHOLD), ToastPlan::Summary);
}
