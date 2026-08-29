//! The kasl-server client, against a local HTTP server.
//!
//! What is under test is how the client reads what comes back, which is where
//! a connection command can mislead: a URL pointing at something that is not a
//! kasl-server, a token the server refuses, an instance whose database is
//! down. Each of those has to arrive as its own message, because each has a
//! different fix.

use kasl::api::kasl_server::KaslServer;
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
