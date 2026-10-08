//! What kasl writes to Jira, against a Jira that is a mock.
//!
//! Two moves - start and close - chosen by status category, a comment, and
//! the question a sync asks Jira about every taken issue that left the inbox.
//! Each test pins what is sent, and just as much what is not: a move kasl
//! could not decide on must leave the issue untouched, because a shared
//! tracker is the one place a wrong guess is seen by everyone.

use kasl::api::Session;
use kasl::api::jira::{Jira, JiraConfig};
use kasl::db::jira_inbox::{JiraInbox, JiraInboxUpsert, LeftReason};
use kasl::libs::config::{Config, JiraInboxConfig};
use kasl::libs::data_storage::DataStorage;
use kasl::libs::jira_bridge::{self, Goal, MoveOptions, Moved};
use kasl::libs::jira_inbox::{ToastBudget, sync_noninteractive};
use kasl::libs::toast_apply::{self, Applied};
use serde_json::{Value, json};
use serial_test::serial;
use tempfile::TempDir;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A throwaway data directory, so the test never touches the machine's own
/// config, database, or cached session.
fn sandbox() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    // SAFETY: tests touching the env are #[serial]
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("LOCALAPPDATA", dir.path());
    }
    dir
}

/// Points the config at the mock, toasts off, with a cached session so every
/// request goes straight to the endpoint under test.
fn configure(server: &MockServer, start_on_take: bool) -> JiraInboxConfig {
    let inbox = JiraInboxConfig {
        notify: false,
        notify_changes: false,
        notify_gone: false,
        start_on_take,
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
    let session = DataStorage::new().get_path(".jira_session_id").unwrap();
    Jira::write_session_id(session.to_str().unwrap(), "JSESSIONID=cafe").unwrap();
    inbox
}

fn client() -> Jira {
    jira_bridge::client().unwrap()
}

/// A status as Jira sends it; the id is the name, which keeps ids distinct.
fn status(name: &str, category: &str) -> Value {
    json!({"id": name, "name": name, "statusCategory": {"id": 2, "key": category, "name": category}})
}

async fn issue_is(server: &MockServer, key: &str, name: &str, category: &str, resolution: Option<&str>) {
    let resolution = resolution.map(|r| json!({"id": "10000", "name": r})).unwrap_or(Value::Null);
    Mock::given(method("GET"))
        .and(path(format!("/rest/api/2/issue/{key}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "key": key,
            "fields": {"status": status(name, category), "resolution": resolution}
        })))
        .mount(server)
        .await;
}

async fn transitions_are(server: &MockServer, key: &str, transitions: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/rest/api/2/issue/{key}/transitions")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "transitions": transitions })))
        .mount(server)
        .await;
}

fn transition(id: &str, name: &str, to: &str, category: &str) -> Value {
    json!({"id": id, "name": name, "to": status(to, category)})
}

/// The bodies of the transition POSTs Jira received, in order.
async fn moves_sent(server: &MockServer, key: &str) -> Vec<Value> {
    let wanted = format!("/rest/api/2/issue/{key}/transitions");
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.method.as_str() == "POST" && r.url.path() == wanted)
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
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
        sort_value: None,
        url: format!("https://jira.example.com/browse/{key}"),
        raw_updated: None,
    }
}

#[tokio::test]
#[serial]
async fn start_takes_the_one_transition_into_progress() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, false);
    issue_is(&server, "KA-1", "Открыт", "new", None).await;
    transitions_are(
        &server,
        "KA-1",
        json!([
            transition("11", "Взять в работу", "В работе", "indeterminate"),
            transition("21", "Решить", "Решена", "done"),
        ]),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/rest/api/2/issue/KA-1/transitions"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let moved = jira_bridge::move_issue(&mut client(), "KA-1", Goal::Start, &MoveOptions::unattended())
        .await
        .unwrap();
    assert_eq!(
        moved,
        Moved::Moved {
            to: "В работе".to_string()
        }
    );
    assert_eq!(moves_sent(&server, "KA-1").await, vec![json!({"transition": {"id": "11"}})]);
}

#[tokio::test]
#[serial]
async fn an_issue_already_in_progress_is_left_alone() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, false);
    issue_is(&server, "KA-2", "In Review", "indeterminate", None).await;
    Mock::given(method("GET"))
        .and(path("/rest/api/2/issue/KA-2/transitions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"transitions": []})))
        .expect(0)
        .mount(&server)
        .await;

    let moved = jira_bridge::move_issue(&mut client(), "KA-2", Goal::Start, &MoveOptions::unattended())
        .await
        .unwrap();
    assert_eq!(
        moved,
        Moved::Already {
            status: "In Review".to_string()
        }
    );
    assert!(moves_sent(&server, "KA-2").await.is_empty(), "an issue already in progress was moved");
}

#[tokio::test]
#[serial]
async fn two_ways_with_nobody_to_ask_move_nothing() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, false);
    issue_is(&server, "KA-3", "Open", "new", None).await;
    transitions_are(
        &server,
        "KA-3",
        json!([
            transition("11", "Start Progress", "In Progress", "indeterminate"),
            transition("12", "Start Review", "In Review", "indeterminate"),
        ]),
    )
    .await;

    let moved = jira_bridge::move_issue(&mut client(), "KA-3", Goal::Start, &MoveOptions::unattended())
        .await
        .unwrap();
    let Moved::Ambiguous { options } = &moved else {
        panic!("two ways in must not be guessed between: {moved:?}");
    };
    assert_eq!(options.len(), 2);
    assert!(moves_sent(&server, "KA-3").await.is_empty(), "an ambiguous move was sent anyway");
    assert!(moved.sentence("KA-3", Goal::Start).contains("kasl inbox start KA-3"));
}

#[tokio::test]
#[serial]
async fn close_comments_first_then_resolves_with_the_named_resolution() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, false);
    issue_is(&server, "KA-4", "In Progress", "indeterminate", None).await;
    let mut resolve = transition("21", "Resolve Issue", "Resolved", "done");
    resolve["fields"] = json!({
        "resolution": {
            "required": true,
            "allowedValues": [{"id": "1", "name": "Fixed"}, {"id": "2", "name": "Won't Fix"}]
        }
    });
    transitions_are(&server, "KA-4", json!([resolve])).await;
    Mock::given(method("POST"))
        .and(path("/rest/api/2/issue/KA-4/comment"))
        .and(body_partial_json(json!({"body": "Shipped in 2.3"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "100"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/rest/api/2/issue/KA-4/transitions"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let options = MoveOptions::from_cli(None, Some("won't fix".to_string()));
    let moved = jira_bridge::close_issue(&mut client(), "KA-4", Some("Shipped in 2.3"), &options).await.unwrap();
    assert_eq!(moved, Moved::Moved { to: "Resolved".to_string() });
    assert_eq!(
        moves_sent(&server, "KA-4").await,
        vec![json!({"transition": {"id": "21"}, "fields": {"resolution": {"id": "2"}}})],
        "the resolution goes by id, matched by name regardless of case"
    );

    // The comment went first: a closed issue may no longer take one.
    let order: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "POST")
        .map(|r| r.url.path().to_string())
        .collect();
    assert_eq!(order, ["/rest/api/2/issue/KA-4/comment", "/rest/api/2/issue/KA-4/transitions"]);
}

#[tokio::test]
#[serial]
async fn a_resolution_nobody_named_is_not_assumed() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, false);
    issue_is(&server, "KA-5", "In Progress", "indeterminate", None).await;
    let mut resolve = transition("21", "Resolve Issue", "Resolved", "done");
    resolve["fields"] = json!({
        "resolution": {"required": true, "allowedValues": [{"id": "1", "name": "Fixed"}, {"id": "2", "name": "Won't Fix"}]}
    });
    transitions_are(&server, "KA-5", json!([resolve])).await;

    let err = jira_bridge::move_issue(&mut client(), "KA-5", Goal::Close, &MoveOptions::unattended())
        .await
        .expect_err("two resolutions and no name must not close the issue");
    let text = err.to_string();
    assert!(text.contains("--resolution") && text.contains("Fixed") && text.contains("Won't Fix"), "{text}");
    assert!(moves_sent(&server, "KA-5").await.is_empty());
}

#[tokio::test]
#[serial]
async fn a_refused_move_reads_as_jiras_own_words() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, false);
    issue_is(&server, "KA-6", "Open", "new", None).await;
    transitions_are(&server, "KA-6", json!([transition("11", "Start Progress", "In Progress", "indeterminate")])).await;
    Mock::given(method("POST"))
        .and(path("/rest/api/2/issue/KA-6/transitions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "errorMessages": [],
            "errors": {"customfield_10100": "Story points are required."}
        })))
        .mount(&server)
        .await;

    let err = jira_bridge::move_issue(&mut client(), "KA-6", Goal::Start, &MoveOptions::unattended())
        .await
        .expect_err("a 400 is a failed move");
    assert!(err.to_string().contains("Story points are required."), "{err}");
}

#[tokio::test]
#[serial]
async fn an_issue_jira_does_not_know_is_missing() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, false);
    Mock::given(method("GET"))
        .and(path("/rest/api/2/issue/KA-7"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"errorMessages": ["Issue Does Not Exist"]})))
        .mount(&server)
        .await;

    let moved = jira_bridge::move_issue(&mut client(), "KA-7", Goal::Close, &MoveOptions::unattended())
        .await
        .unwrap();
    assert_eq!(moved, Moved::Missing);
    assert!(!moved.arrived());
}

#[tokio::test]
#[serial]
async fn a_take_starts_the_issue_when_asked_to() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, true);
    JiraInbox::new().unwrap().upsert_batch(&[seed("KA-8")]).unwrap();
    issue_is(&server, "KA-8", "Open", "new", None).await;
    transitions_are(&server, "KA-8", json!([transition("11", "Start Progress", "In Progress", "indeterminate")])).await;
    Mock::given(method("POST"))
        .and(path("/rest/api/2/issue/KA-8/transitions"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let outcome = toast_apply::take("KA-8", &MoveOptions::unattended()).await.unwrap();
    assert_eq!(
        outcome,
        Applied::Taken {
            started: Some(Ok(Moved::Moved { to: "In Progress".to_string() }))
        }
    );
    assert!(outcome.summary("KA-8").contains("In Progress"), "the answer says where the issue went");

    // The inbox knows at once, so the next poll does not toast the person's
    // own move back at them as a status change.
    let item = JiraInbox::new().unwrap().get_by_key("KA-8").unwrap().unwrap();
    assert_eq!(item.status_id.as_deref(), Some("In Progress"));
    assert_eq!(item.status_name, "In Progress");
}

#[tokio::test]
#[serial]
async fn a_take_stands_when_jira_does_not_answer() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, true);
    JiraInbox::new().unwrap().upsert_batch(&[seed("KA-9")]).unwrap();
    Mock::given(method("GET"))
        .and(path("/rest/api/2/issue/KA-9"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let outcome = toast_apply::take("KA-9", &MoveOptions::unattended()).await.unwrap();
    let Applied::Taken { started: Some(Err(why)) } = &outcome else {
        panic!("a Jira that does not answer is reported, not swallowed: {outcome:?}");
    };
    assert!(why.contains("kasl inbox start KA-9"), "the answer names the retry: {why}");
    assert!(
        JiraInbox::new().unwrap().get_by_key("KA-9").unwrap().unwrap().taken_at.is_some(),
        "the take itself must stand whatever Jira says"
    );
}

#[tokio::test]
#[serial]
async fn a_take_does_not_touch_jira_unless_asked() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, false);
    JiraInbox::new().unwrap().upsert_batch(&[seed("KA-10")]).unwrap();

    let outcome = toast_apply::take("KA-10", &MoveOptions::unattended()).await.unwrap();
    assert_eq!(outcome, Applied::Taken { started: None });
    assert!(
        server.received_requests().await.unwrap_or_default().is_empty(),
        "start_on_take is off: Jira must hear nothing"
    );
}

/// Search answers with an empty inbox: everything seeded has left.
async fn inbox_is_empty(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "startAt": 0, "maxResults": 100, "total": 0, "issues": []
        })))
        .mount(server)
        .await;
}

#[tokio::test]
#[serial]
async fn a_sync_learns_why_taken_issues_left() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server, false);
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[seed("KA-11"), seed("KA-12"), seed("KA-13")]).unwrap();
    toast_apply::take("KA-11", &MoveOptions::unattended()).await.unwrap();
    toast_apply::take("KA-12", &MoveOptions::unattended()).await.unwrap();
    inbox_is_empty(&server).await;
    issue_is(&server, "KA-11", "Done", "done", Some("Done")).await;
    issue_is(&server, "KA-12", "Open", "new", None).await;
    // KA-13 was never taken: why it left is nobody's question.
    Mock::given(method("GET"))
        .and(path("/rest/api/2/issue/KA-13"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert_eq!(
        outcome.resolved.iter().map(|r| r.issue_key.as_str()).collect::<Vec<_>>(),
        ["KA-11"],
        "only the resolved issue with an open task is offered"
    );
    assert_eq!(outcome.resolved[0].resolution.as_deref(), Some("Done"));

    let now = chrono::Local::now().naive_local();
    let resolved = db.get_by_key("KA-11").unwrap().unwrap();
    assert_eq!(resolved.left_reason, Some(LeftReason::Resolved));
    assert_eq!(resolved.badge(now).as_deref(), Some("resolved"));
    let moved = db.get_by_key("KA-12").unwrap().unwrap();
    assert_eq!(moved.left_reason, Some(LeftReason::Moved));
    assert_eq!(moved.badge(now).as_deref(), Some("gone"));
    assert_eq!(db.get_by_key("KA-13").unwrap().unwrap().left_reason, None);

    // Asked once: the next sync has nothing left to ask, and offers nothing again.
    let again = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert!(again.resolved.is_empty(), "a resolution is offered once, not every poll: {:?}", again.resolved);
}

#[tokio::test]
#[serial]
async fn a_resolved_issue_whose_task_is_done_is_not_offered() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server, false);
    JiraInbox::new().unwrap().upsert_batch(&[seed("KA-14")]).unwrap();
    toast_apply::take("KA-14", &MoveOptions::unattended()).await.unwrap();
    toast_apply::complete("KA-14").unwrap();
    inbox_is_empty(&server).await;
    issue_is(&server, "KA-14", "Done", "done", Some("Done")).await;

    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert!(outcome.resolved.is_empty(), "the work is done on both sides: {:?}", outcome.resolved);
    let item = JiraInbox::new().unwrap().get_by_key("KA-14").unwrap().unwrap();
    assert_eq!(item.left_reason, Some(LeftReason::Resolved), "the reason is kept all the same");
}

#[tokio::test]
#[serial]
async fn a_departure_jira_could_not_explain_is_asked_about_again() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server, false);
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[seed("KA-15")]).unwrap();
    toast_apply::take("KA-15", &MoveOptions::unattended()).await.unwrap();
    inbox_is_empty(&server).await;
    let busy = Mock::given(method("GET"))
        .and(path("/rest/api/2/issue/KA-15"))
        .respond_with(ResponseTemplate::new(503))
        .mount_as_scoped(&server)
        .await;

    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert!(outcome.resolved.is_empty());
    assert_eq!(db.get_by_key("KA-15").unwrap().unwrap().left_reason, None, "no answer is no answer");
    drop(busy);

    issue_is(&server, "KA-15", "Closed", "done", Some("Fixed")).await;
    let outcome = sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();
    assert_eq!(outcome.resolved.len(), 1, "the next poll asks again and gets the answer");
    assert_eq!(db.get_by_key("KA-15").unwrap().unwrap().resolution.as_deref(), Some("Fixed"));
}

#[tokio::test]
#[serial]
async fn an_issue_that_comes_back_forgets_why_it_left() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    let inbox = configure(&server, false);
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[seed("KA-16")]).unwrap();
    toast_apply::take("KA-16", &MoveOptions::unattended()).await.unwrap();
    db.mark_gone(&[]).unwrap();
    db.set_left("KA-16", LeftReason::Resolved, Some("Done")).unwrap();

    // Reopened in Jira: it is back in the poll.
    Mock::given(method("GET"))
        .and(path("/rest/api/2/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "startAt": 0, "maxResults": 100, "total": 1,
            "issues": [{
                "id": "1", "key": "KA-16",
                "fields": {"summary": "Summary for KA-16", "status": {"id": "10", "name": "Reopened"}, "priority": null}
            }]
        })))
        .mount(&server)
        .await;
    sync_noninteractive(&inbox, &mut ToastBudget::new()).await.unwrap();

    let item = db.get_by_key("KA-16").unwrap().unwrap();
    assert!(item.gone_at.is_none());
    assert_eq!((item.left_reason, item.resolution), (None, None), "a reopened issue is not resolved");
}

/// A close that cannot happen must not leave its comment behind: the retry
/// with `--to` or `--resolution` would post the same note a second time.
#[tokio::test]
#[serial]
async fn a_close_that_cannot_happen_leaves_no_comment() {
    let _dir = sandbox();
    let server = MockServer::start().await;
    configure(&server, false);
    issue_is(&server, "KA-17", "In Progress", "indeterminate", None).await;
    let mut resolve = transition("21", "Resolve Issue", "Resolved", "done");
    resolve["fields"] = json!({
        "resolution": {"required": true, "allowedValues": [{"id": "1", "name": "Fixed"}, {"id": "2", "name": "Won't Fix"}]}
    });
    transitions_are(&server, "KA-17", json!([resolve, transition("31", "Close Issue", "Closed", "done")])).await;
    Mock::given(method("POST"))
        .and(path("/rest/api/2/issue/KA-17/comment"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&server)
        .await;

    // Two ways to close, nobody to choose.
    let moved = jira_bridge::close_issue(&mut client(), "KA-17", Some("Fixed in 2.3"), &MoveOptions::unattended())
        .await
        .unwrap();
    assert!(matches!(moved, Moved::Ambiguous { .. }), "{moved:?}");

    // The way named, but the resolution it needs is not.
    let options = MoveOptions {
        to: Some("Resolved".to_string()),
        ..MoveOptions::unattended()
    };
    let err = jira_bridge::close_issue(&mut client(), "KA-17", Some("Fixed in 2.3"), &options).await;
    assert!(err.is_err(), "a resolution nobody named stops the close");
    assert!(moves_sent(&server, "KA-17").await.is_empty());
}
