//! The kasl-server client, against a local HTTP server.
//!
//! What is under test is how the client reads what comes back, which is where
//! a connection command can mislead: a URL pointing at something that is not a
//! kasl-server, a token the server refuses, an instance whose database is
//! down. Each of those has to arrive as its own message, because each has a
//! different fix.

use chrono::{DateTime, NaiveDate};
use kasl::api::kasl_server::{DayResult, DayUpload, KaslServer};
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

/// A day for `date`, so a batch can carry several distinguishable ones.
fn a_day_on(date: &str) -> DayUpload {
    DayUpload {
        date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
        started_at: DateTime::parse_from_rfc3339(&format!("{}T09:00:00-03:00", date)).unwrap(),
        ended_at: Some(DateTime::parse_from_rfc3339(&format!("{}T18:00:00-03:00", date)).unwrap()),
        pauses: vec![],
        tasks: vec![],
        tasks_are_complete: true,
    }
}

#[tokio::test]
async fn a_batch_reports_each_day_separately() {
    let server = MockServer::start().await;
    // The shape the server actually answers with: a per-day list, not one
    // verdict for the request (ADR 0005 in kasl-server).
    Mock::given(method("POST"))
        .and(path("/api/v1/days/batch"))
        .and(header("authorization", "Bearer token-kirill"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accepted": 1,
            "rejected": 1,
            "results": [
                {
                    "status": "accepted",
                    "workday_id": "0f7b6f0e-4f2f-4a3e-9a2c-6a2b1c3d4e5f",
                    "date": "2026-08-30",
                    "pauses": 2,
                    "tasks": 3,
                    "deleted_tasks": 0,
                    "privacy_level": "full"
                },
                {
                    "status": "rejected",
                    "date": "2026-08-31",
                    "error": "tasks[0]: name is empty"
                }
            ]
        })))
        .mount(&server)
        .await;

    let result = client_for(&server)
        .upload_batch("token-kirill", &[a_day_on("2026-08-30"), a_day_on("2026-08-31")])
        .await
        .unwrap();

    assert_eq!(result.accepted, 1);
    assert_eq!(result.rejected, 1);
    assert_eq!(result.results.len(), 2);

    match &result.results[0] {
        DayResult::Accepted { day } => {
            assert_eq!(day.date, NaiveDate::from_ymd_opt(2026, 8, 30).unwrap());
            assert_eq!(day.tasks, 3);
        }
        other => panic!("the first day was accepted, not {:?}", other),
    }

    match &result.results[1] {
        DayResult::Rejected { date, error } => {
            assert_eq!(*date, NaiveDate::from_ymd_opt(2026, 8, 31).unwrap());
            assert!(error.contains("name is empty"), "the reason should survive: {}", error);
        }
        other => panic!("the second day was rejected, not {:?}", other),
    }
}

#[tokio::test]
async fn a_batch_that_answers_200_with_rejections_is_not_read_as_success() {
    let server = MockServer::start().await;
    // The trap ADR 0005 names outright: the status describes the request,
    // which was processed. A client reading only the status believes two days
    // arrived and drops both from its queue.
    Mock::given(method("POST"))
        .and(path("/api/v1/days/batch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accepted": 0,
            "rejected": 2,
            "results": [
                {"status": "rejected", "date": "2026-08-30", "error": "started_at has no offset"},
                {"status": "rejected", "date": "2026-08-31", "error": "started_at has no offset"}
            ]
        })))
        .mount(&server)
        .await;

    let result = client_for(&server)
        .upload_batch("token-kirill", &[a_day_on("2026-08-30"), a_day_on("2026-08-31")])
        .await
        .expect("the request itself succeeded");

    assert_eq!(result.accepted, 0, "the request was fine; the days were not");
    assert!(
        result.results.iter().all(|day| matches!(day, DayResult::Rejected { .. })),
        "both days should read as rejected"
    );
}

#[tokio::test]
async fn a_batch_too_large_is_refused_without_a_retry() {
    let server = MockServer::start().await;
    // 413 past KASL_MAX_BATCH_DAYS. Retrying the identical request would get
    // the same answer forever; the caller has to send fewer days.
    Mock::given(method("POST"))
        .and(path("/api/v1/days/batch"))
        .respond_with(ResponseTemplate::new(413).set_body_json(serde_json::json!({
            "error": "a batch carries at most 30 days; split the backlog"
        })))
        .mount(&server)
        .await;

    let error = client_for(&server).upload_batch("token-kirill", &[a_day()]).await.unwrap_err();

    assert!(!error.is_retryable(), "the same oversized request will never be accepted");
    assert!(error.to_string().contains("split the backlog"), "the fix should survive: {}", error);
}

#[tokio::test]
async fn a_batch_meeting_a_server_error_keeps_every_day() {
    let server = MockServer::start().await;
    // The server aborts a batch it failed on rather than calling the days
    // rejected, so this must read as "try later" - not as data to discard.
    Mock::given(method("POST"))
        .and(path("/api/v1/days/batch"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({"error": "database is unavailable"})))
        .mount(&server)
        .await;

    let error = client_for(&server)
        .upload_batch("token-kirill", &[a_day_on("2026-08-30"), a_day_on("2026-08-31")])
        .await
        .unwrap_err();

    assert!(error.is_retryable(), "a server that failed on the batch is worth asking again");
}

#[tokio::test]
async fn a_rate_limited_batch_is_worth_repeating() {
    let server = MockServer::start().await;
    // 429 sits inside the 4xx range and is the one exception to "4xx is
    // final". Reading it by range alone would throw a backlog away for
    // sending too fast.
    Mock::given(method("POST"))
        .and(path("/api/v1/days/batch"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({"error": "too many requests"})))
        .mount(&server)
        .await;

    let error = client_for(&server).upload_batch("token-kirill", &[a_day()]).await.unwrap_err();

    assert!(error.is_retryable(), "a rate limit is a wait, not a refusal of the data");
}

/// A manifest body in the shape the server sends it.
fn manifest_body(level: &str) -> serde_json::Value {
    serde_json::json!({
        "level": level,
        "summary": "This server stores your working hours, when you were interrupted, and the names of tasks you logged - but none of the text you typed about them.",
        "stored": [
            { "what": "workdays", "detail": "the date, when the day started, when it ended" },
            { "what": "pauses", "detail": "each interruption: when it began and how long it lasted" },
            { "what": "tasks", "detail": "what you logged: the name and how complete you marked it" }
        ],
        "never_collected": ["keystrokes or what you type", "window titles", "screenshots or camera images"],
        "visible_to": ["you, in your own account", "the manager of your department"],
        "retention": "Kept for as long as the installation keeps it: there is no automatic deletion.",
        "on_change": "Changing this setting affects what arrives from now on.",
        "updated_at": "2026-09-02T11:30:00Z"
    })
}

#[tokio::test]
async fn the_manifest_is_read_as_the_server_wrote_it() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/privacy/agent"))
        .and(header("authorization", "Bearer token-kirill"))
        .respond_with(ResponseTemplate::new(200).set_body_json(manifest_body("moderate")))
        .mount(&server)
        .await;

    let manifest = client_for(&server).privacy("token-kirill").await.unwrap();

    // Every field comes from the server. The point of the command is that
    // kasl repeats the installation's own words rather than describing the
    // server from this side, so a field silently defaulted here would be kasl
    // inventing a promise.
    assert_eq!(manifest.level, "moderate");
    assert!(manifest.summary.contains("none of the text you typed"), "summary: {}", manifest.summary);
    assert_eq!(manifest.stored.len(), 3);
    assert_eq!(manifest.stored[1].what, "pauses");
    assert!(manifest.stored[1].detail.contains("each interruption"), "detail: {}", manifest.stored[1].detail);
    assert_eq!(manifest.never_collected.len(), 3);
    assert!(manifest.never_collected.iter().any(|line| line.contains("window titles")));
    assert_eq!(manifest.visible_to.len(), 2);
    assert!(manifest.retention.contains("no automatic deletion"), "retention: {}", manifest.retention);
    assert!(manifest.on_change.contains("from now on"), "on_change: {}", manifest.on_change);
    assert!(manifest.updated_at.is_some(), "the server sent a timestamp and it should survive parsing");
}

#[tokio::test]
async fn a_manifest_without_a_timestamp_is_still_read() {
    let server = MockServer::start().await;
    // A server that does not record when the level was set. The field being
    // absent must not cost the rest of the manifest, which is the part the
    // employee is reading.
    let mut body = manifest_body("full");
    body.as_object_mut().unwrap().remove("updated_at");
    Mock::given(method("GET"))
        .and(path("/api/v1/privacy/agent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let manifest = client_for(&server).privacy("token-kirill").await.unwrap();

    assert_eq!(manifest.level, "full");
    assert!(manifest.updated_at.is_none(), "an absent timestamp is absent, not invented");
}

#[tokio::test]
async fn a_refused_token_on_the_manifest_is_named_as_a_token_problem() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/privacy/agent"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"error": "unknown token"})))
        .mount(&server)
        .await;

    let error = client_for(&server).privacy("token-kirill").await.unwrap_err().to_string();

    assert!(error.contains("rejected this token"), "unexpected error: {}", error);
}

#[tokio::test]
async fn a_server_with_no_manifest_route_says_so_rather_than_showing_nothing() {
    let server = MockServer::start().await;
    // An installation older than the route. Read as "no manifest" it would
    // look like a server that keeps nothing, which is the one wrong reading
    // this failure must never produce.
    Mock::given(method("GET"))
        .and(path("/api/v1/privacy/agent"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({"error": "no such endpoint"})))
        .mount(&server)
        .await;

    let error = client_for(&server).privacy("token-kirill").await.unwrap_err().to_string();

    assert!(error.contains("does not publish a privacy manifest"), "unexpected error: {}", error);
    assert!(error.contains("0.10.0"), "the error should name the version that has it: {}", error);
}

#[tokio::test]
async fn a_manifest_that_is_not_a_manifest_is_not_read_as_one() {
    let server = MockServer::start().await;
    // A proxy answering 200 with a page. Showing that as a privacy manifest
    // would be the product lying about the thing it exists to be honest about.
    Mock::given(method("GET"))
        .and(path("/api/v1/privacy/agent"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>OK</html>"))
        .mount(&server)
        .await;

    let error = client_for(&server).privacy("token-kirill").await.unwrap_err().to_string();

    assert!(error.contains("cannot read the server's privacy manifest"), "unexpected error: {}", error);
}

#[tokio::test]
async fn whoami_carries_the_versions_that_decide_compatibility() {
    let server = MockServer::start().await;
    // `server status` prints these two. They arrive from the server rather
    // than being assumed, so an agent talking to an installation it does not
    // match can say which pair it is looking at.
    Mock::given(method("GET"))
        .and(path("/api/v1/agent/whoami"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "user_name": "Kirill Lakhtachev",
            "agent_name": "laptop",
            "api_version": "v1",
            "server_version": "0.22.2"
        })))
        .mount(&server)
        .await;

    let identity = client_for(&server).identify("token-kirill").await.unwrap();

    assert_eq!(identity.server_version, "0.22.2");
    assert_eq!(identity.api_version, "v1");
}
