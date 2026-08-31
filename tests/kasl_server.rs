//! The kasl-server client, against a local HTTP server.
//!
//! What is under test is how the client reads what comes back, which is where
//! a connection command can mislead: a URL pointing at something that is not a
//! kasl-server, a token the server refuses, an instance whose database is
//! down. Each of those has to arrive as its own message, because each has a
//! different fix.

use chrono::{DateTime, NaiveDate};
use kasl::api::kasl_server::{DayUpload, KaslServer};
use kasl::libs::config::KaslServerConfig;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A client pointed at the mock server.
fn client_for(server: &MockServer) -> KaslServer {
    KaslServer::new(&KaslServerConfig {
        url: server.uri(),
        ca_certificate: None,
    })
    .expect("the client should build for a plain http url")
}

#[tokio::test]
async fn health_reports_the_server_version() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "version": "0.14.1",
            "database": "ok"
        })))
        .mount(&server)
        .await;

    let health = client_for(&server).health().await.unwrap();

    assert_eq!(health.status, "ok");
    assert_eq!(health.version, "0.14.1");
    assert_eq!(health.database, "ok");
}

#[tokio::test]
async fn a_server_that_answers_but_is_not_kasl_server_is_named_as_such() {
    let server = MockServer::start().await;
    // A parked domain, a proxy, a different app on that port: all answer 200
    // with something that is not a health report. Reading that as "connected"
    // is the failure this guards.
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html><body>It works!</body></html>"))
        .mount(&server)
        .await;

    let error = client_for(&server).health().await.unwrap_err().to_string();

    assert!(error.contains("not like a kasl-server"), "unexpected error: {}", error);
}

#[tokio::test]
async fn an_unhealthy_database_is_reported_rather_than_hidden() {
    let server = MockServer::start().await;
    // The server answers 503 when it cannot reach its database. The client
    // must surface the status rather than treat a reachable process as a
    // working server.
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
            "status": "degraded",
            "version": "0.14.1",
            "database": "unreachable"
        })))
        .mount(&server)
        .await;

    let error = client_for(&server).health().await.unwrap_err().to_string();

    assert!(error.contains("503"), "the error should carry the status: {}", error);
}

#[tokio::test]
async fn whoami_names_the_employee_behind_the_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/agent/whoami"))
        .and(header("authorization", "Bearer token-kirill"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_name": "kirill",
            "agent_name": "laptop",
            "api_version": "v1",
            "server_version": "0.14.1"
        })))
        .mount(&server)
        .await;

    let identity = client_for(&server).identify("token-kirill").await.unwrap();

    assert_eq!(identity.user_name, "kirill");
    assert_eq!(identity.agent_name, "laptop");
    assert_eq!(identity.api_version, "v1");
    assert_eq!(identity.server_version, "0.14.1");
}

#[tokio::test]
async fn the_token_travels_as_a_bearer_header() {
    let server = MockServer::start().await;
    // Mounted with the header matcher only: a request without it matches
    // nothing and the mock server answers 404, so this fails if the token is
    // sent some other way - or not at all.
    Mock::given(method("GET"))
        .and(path("/api/v1/agent/whoami"))
        .and(header("authorization", "Bearer token-kirill"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_name": "kirill",
            "agent_name": "laptop",
            "api_version": "v1",
            "server_version": "0.14.1"
        })))
        .mount(&server)
        .await;

    assert!(client_for(&server).identify("token-kirill").await.is_ok());
    // The same call with a different token must not match the mock.
    assert!(client_for(&server).identify("someone-elses-token").await.is_err());
}

#[tokio::test]
async fn a_refused_token_is_reported_as_a_token_problem() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/agent/whoami"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": "the token is not recognized, has been revoked, or its user is deactivated"
        })))
        .mount(&server)
        .await;

    let error = client_for(&server).identify("stale-token").await.unwrap_err().to_string();

    // The distinction that matters to whoever is connecting: fix the token,
    // not the URL or the network.
    assert!(error.contains("rejected this token"), "unexpected error: {}", error);
    assert!(error.contains("revoked"), "the message should suggest what went wrong: {}", error);
}

#[tokio::test]
async fn an_unexpected_status_is_not_read_as_success() {
    let server = MockServer::start().await;
    // A reverse proxy in front of a stopped server, for instance. Neither a
    // valid identity nor an authentication problem.
    Mock::given(method("GET"))
        .and(path("/api/v1/agent/whoami"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&server)
        .await;

    let error = client_for(&server).identify("token-kirill").await.unwrap_err().to_string();

    assert!(error.contains("502"), "the error should carry the status: {}", error);
}

#[tokio::test]
async fn a_server_that_is_not_listening_is_reported_by_url() {
    // A URL that resolves but has nothing behind it - the common typo, and a
    // different fix from a bad token.
    let config = KaslServerConfig {
        // Port 1 is reserved and never has a listener.
        url: "http://127.0.0.1:1".to_string(),
        ca_certificate: None,
    };

    let error = KaslServer::new(&config).unwrap().health().await.unwrap_err().to_string();

    assert!(error.contains("cannot reach"), "unexpected error: {}", error);
    assert!(error.contains("127.0.0.1:1"), "the error should name the url: {}", error);
}

#[tokio::test]
async fn a_url_with_a_trailing_slash_still_addresses_the_endpoints() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "version": "0.14.1",
            "database": "ok"
        })))
        .mount(&server)
        .await;

    // Without normalisation this would request `//health`, which a strict
    // router does not match.
    let client = KaslServer::new(&KaslServerConfig {
        url: format!("{}/", server.uri()),
        ca_certificate: None,
    })
    .unwrap();

    assert!(client.health().await.is_ok());
}

/// A minimal day, enough for the server to have something to answer about.
fn a_day() -> DayUpload {
    DayUpload {
        date: NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
        started_at: DateTime::parse_from_rfc3339("2026-08-31T09:00:00-03:00").unwrap(),
        ended_at: Some(DateTime::parse_from_rfc3339("2026-08-31T18:00:00-03:00").unwrap()),
        pauses: vec![],
        tasks: vec![],
        tasks_are_complete: true,
    }
}

/// What the server sends back for an accepted day.
fn accepted_body(deleted_tasks: u64) -> serde_json::Value {
    serde_json::json!({
        "workday_id": "0f7b6f0e-4f2f-4a3e-9a2c-6a2b1c3d4e5f",
        "date": "2026-08-31",
        "pauses": 2,
        "tasks": 3,
        "deleted_tasks": deleted_tasks,
        "privacy_level": "full"
    })
}

#[tokio::test]
async fn an_accepted_day_is_reported_with_what_the_server_stored() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .and(header("authorization", "Bearer token-kirill"))
        .respond_with(ResponseTemplate::new(200).set_body_json(accepted_body(1)))
        .mount(&server)
        .await;

    let accepted = client_for(&server).upload_day("token-kirill", &a_day()).await.unwrap();

    assert_eq!(accepted.pauses, 2);
    assert_eq!(accepted.tasks, 3);
    // The visible consequence of `tasks_are_complete`: a task deleted here
    // was deleted there. Silence about it would hide a deletion.
    assert_eq!(accepted.deleted_tasks, 1);
}

#[tokio::test]
async fn the_payload_carries_offsets_and_the_authoritative_flag() {
    // What the wire actually gets. The server refuses an instant without an
    // offset and reads a missing flag as "do not delete anything" (ADR 0003,
    // ADR 0005), so both have to survive serialisation - a detail no
    // round-trip through our own types would catch.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .respond_with(ResponseTemplate::new(200).set_body_json(accepted_body(0)))
        .mount(&server)
        .await;

    client_for(&server).upload_day("token-kirill", &a_day()).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();

    assert_eq!(body["date"], "2026-08-31", "the agent's own date, not one derived from the instant");
    assert_eq!(body["started_at"], "2026-08-31T09:00:00-03:00");
    assert_eq!(body["ended_at"], "2026-08-31T18:00:00-03:00");
    assert_eq!(body["tasks_are_complete"], true);
}

#[tokio::test]
async fn an_open_day_omits_the_end_rather_than_sending_null() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .respond_with(ResponseTemplate::new(200).set_body_json(accepted_body(0)))
        .mount(&server)
        .await;

    let mut day = a_day();
    day.ended_at = None;
    client_for(&server).upload_day("token-kirill", &day).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();

    assert!(body.get("ended_at").is_none(), "an unfinished day carries no end: {}", body);
}

#[tokio::test]
async fn a_refused_payload_is_not_worth_retrying() {
    let server = MockServer::start().await;
    // The server's own rule: 4xx will never be accepted as sent (ADR 0005).
    // A queue that retried this would ask forever.
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({"error": "tasks[0]: name is empty"})))
        .mount(&server)
        .await;

    let error = client_for(&server).upload_day("token-kirill", &a_day()).await.unwrap_err();

    assert!(!error.is_retryable(), "a 400 must not be queued for a retry");
    // The server's sentence, not the JSON wrapper around it: it names what to
    // fix, and the wrapper buries it.
    assert!(error.to_string().contains("tasks[0]: name is empty"), "unexpected error: {}", error);
}

#[tokio::test]
async fn a_failure_states_the_status_once() {
    let server = MockServer::start().await;
    // The server's own error text opens with the status, and the message the
    // user reads adds it too. Adding it a third time in between produced
    // "the server refused the day (401 Unauthorized): 401 Unauthorized: the
    // token is not recognized" - three problems where there is one. Found by
    // running the real thing: every test here asserted the status was
    // present, and none that it appeared once.
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"error": "401 Unauthorized: the token is not recognized"})))
        .mount(&server)
        .await;

    let error = client_for(&server).upload_day("stale-token", &a_day()).await.unwrap_err().to_string();

    assert_eq!(
        error.matches("401").count(),
        2,
        "the status belongs to the wrapper and the server's own sentence, nowhere else: {}",
        error
    );
}

#[tokio::test]
async fn a_failure_without_a_body_still_says_what_happened() {
    let server = MockServer::start().await;
    // A bare status and no explanation. The message has to carry the status
    // itself here, or there is nothing at all to go on.
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .respond_with(ResponseTemplate::new(418))
        .mount(&server)
        .await;

    let error = client_for(&server).upload_day("token-kirill", &a_day()).await.unwrap_err().to_string();

    assert!(error.contains("418"), "the status is all there is to report: {}", error);
}

#[tokio::test]
async fn a_rejected_token_is_not_worth_retrying() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let error = client_for(&server).upload_day("stale-token", &a_day()).await.unwrap_err();

    assert!(!error.is_retryable(), "a revoked token is fixed by reconnecting, not by waiting");
    assert!(error.to_string().contains("401"), "the error should carry the status: {}", error);
}

#[tokio::test]
async fn a_server_that_could_not_answer_keeps_the_day() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let error = client_for(&server).upload_day("token-kirill", &a_day()).await.unwrap_err();

    assert!(error.is_retryable(), "a 5xx means the server could not answer this time");
}

#[tokio::test]
async fn being_rate_limited_is_worth_retrying_despite_being_a_4xx() {
    let server = MockServer::start().await;
    // 429 sits inside the 4xx range and is the one exception to it: the
    // payload is fine, the server is only asking for a pause. Treating it
    // like the other 4xx would throw away a day that would have been accepted.
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let error = client_for(&server).upload_day("token-kirill", &a_day()).await.unwrap_err();

    assert!(error.is_retryable(), "429 asks for a later attempt, not for the day to be dropped");
}

#[tokio::test]
async fn an_unreachable_server_keeps_the_day() {
    let client = KaslServer::new(&KaslServerConfig {
        // Port 1 is reserved and never has a listener.
        url: "http://127.0.0.1:1".to_string(),
        ca_certificate: None,
    })
    .unwrap();

    let error = client.upload_day("token-kirill", &a_day()).await.unwrap_err();

    assert!(error.is_retryable(), "nothing was answered, so nothing was written there");
    assert!(error.to_string().contains("127.0.0.1:1"), "the error should name the url: {}", error);
}

#[tokio::test]
async fn a_success_that_is_not_a_day_report_is_not_read_as_stored() {
    let server = MockServer::start().await;
    // A proxy or a different app answering 200 on that path. Reading it as an
    // accepted day would drop the day from a queue that never delivered it.
    Mock::given(method("POST"))
        .and(path("/api/v1/days"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>OK</html>"))
        .mount(&server)
        .await;

    let error = client_for(&server).upload_day("token-kirill", &a_day()).await.unwrap_err();

    assert!(!error.is_retryable(), "an address answering for something else will not come good on a retry");
    assert!(error.to_string().contains("unreadably"), "unexpected error: {}", error);
}
