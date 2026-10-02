//! The pulse: what the watcher tells kasl-server about right now.
//!
//! Three layers, each against a mock server or the real binary rather than a
//! fake: the client reading the server's answers, the watcher's loop deciding
//! when to send and what to claim, and the commands a person types to switch
//! it and read how it is going.
//!
//! The ways a pulse goes wrong that these are written against:
//!
//! * a pulse sent while it is off, or sent to a server the employee did not
//!   agree to report to;
//! * a claim that does not match what the watcher sees - "working" on a break,
//!   "working" after the day was closed;
//! * a server asked every few seconds - offline, or after it refused;
//! * a status that reads healthy while nothing is being sent.

#[cfg(test)]
mod tests {
    use chrono::{Duration, Local, Utc};
    use kasl::api::kasl_server::{AgentState, KaslServer, UploadError};
    use kasl::db::server_pulse::ServerPulse;
    use kasl::db::workdays::Workdays;
    use kasl::libs::config::{Config, KaslServerConfig};
    use kasl::libs::pulse::{Beat, DEFAULT_INTERVAL, Pulser, REFUSED_RETRY, beat};
    use serial_test::serial;
    use std::path::Path;
    use std::process::{Command, Output, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tempfile::TempDir;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    const HEARTBEAT: &str = "/api/v1/agent/heartbeat";

    /// Points the data directory at a fresh temporary one, and keeps it alive.
    #[must_use]
    fn sandbox() -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        // SAFETY: every test here is #[serial], so nothing else is reading the
        // environment while it is being set.
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
        }
        temp_dir
    }

    fn server_config(url: &str, pulse: bool) -> KaslServerConfig {
        KaslServerConfig {
            url: url.to_string(),
            ca_certificate: None,
            pulse,
        }
    }

    fn connect(url: &str, pulse: bool) {
        let config = Config {
            kasl_server: Some(server_config(url, pulse)),
            ..Default::default()
        };
        config.save().unwrap();
    }

    fn client_for(server: &MockServer) -> KaslServer {
        KaslServer::new(&server_config(&server.uri(), true)).unwrap()
    }

    /// The server's answer to an accepted pulse, echoing `state`.
    fn accepted(state: &str) -> ResponseTemplate {
        ResponseTemplate::new(202).set_body_json(serde_json::json!({
            "interval_seconds": 60,
            "stale_after_seconds": 180,
            "state": state,
            "clock_skew_seconds": 2,
            "notifications": 0
        }))
    }

    /// Answers whatever state was claimed, as the real server does.
    struct Echo;

    impl wiremock::Respond for Echo {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            accepted(body["state"].as_str().unwrap())
        }
    }

    /// The states claimed so far, in order.
    async fn claims(server: &MockServer) -> Vec<String> {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == HEARTBEAT)
            .map(|request| {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                body["state"].as_str().unwrap().to_string()
            })
            .collect()
    }

    fn pulser(in_pause: &Arc<AtomicBool>) -> Pulser {
        Pulser::with_token(Arc::clone(in_pause), Box::new(|| Some("token-kirill".to_string())))
    }

    fn start_today() {
        Workdays::new().unwrap().insert_start(Local::now().date_naive()).unwrap();
    }

    // === The client ===

    #[tokio::test]
    async fn a_pulse_carries_the_state_and_the_moment_and_nothing_else() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(HEARTBEAT))
            .and(header("authorization", "Bearer token-kirill"))
            .respond_with(accepted("working"))
            .expect(1)
            .mount(&server)
            .await;

        let at = Local::now().fixed_offset();
        let answer = client_for(&server)
            .heartbeat(
                "token-kirill",
                &kasl::api::kasl_server::Pulse {
                    state: AgentState::Working,
                    at,
                },
            )
            .await
            .unwrap();

        assert_eq!(answer.interval_seconds, 60);
        assert_eq!(answer.stale_after_seconds, 180);
        assert_eq!(answer.state, AgentState::Working);

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        // The whole body, key by key: a task name or a break reason riding
        // along would turn the pulse into a live feed of what someone is doing.
        let keys: Vec<&String> = body.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["at", "state"], "the pulse carries more than the state and the moment: {body}");
        assert_eq!(body["state"], "working");
        // With the offset, like every instant this agent sends.
        let sent_at = chrono::DateTime::parse_from_rfc3339(body["at"].as_str().unwrap()).unwrap();
        assert_eq!(sent_at, at);
    }

    #[tokio::test]
    async fn a_clock_the_server_refuses_is_a_refusal_with_its_reason() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(HEARTBEAT))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "the pulse is stamped 75 s ahead of this server; check the machine's clock"
            })))
            .mount(&server)
            .await;

        let pulse = kasl::api::kasl_server::Pulse {
            state: AgentState::Working,
            at: Local::now().fixed_offset(),
        };
        match client_for(&server).heartbeat("token", &pulse).await {
            Err(UploadError::Rejected { message, .. }) => assert!(message.contains("check the machine's clock"), "{message}"),
            other => panic!("a 400 should be a refusal carrying the server's sentence, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_server_older_than_the_route_is_named_as_such() {
        // kasl-server 0.14.1 to 0.16.x accepts a connection and answers the
        // pulse with 404. That is the administrator's to fix, and the message
        // has to say so rather than read as a broken network.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(HEARTBEAT))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({"error": "no such endpoint"})))
            .mount(&server)
            .await;

        let pulse = kasl::api::kasl_server::Pulse {
            state: AgentState::Idle,
            at: Local::now().fixed_offset(),
        };
        match client_for(&server).heartbeat("token", &pulse).await {
            Err(UploadError::Rejected { message, .. }) => assert!(message.contains("0.17.0"), "{message}"),
            other => panic!("a 404 should be a refusal naming the version, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_server_that_cannot_answer_is_worth_asking_again() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(HEARTBEAT))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let pulse = kasl::api::kasl_server::Pulse {
            state: AgentState::Working,
            at: Local::now().fixed_offset(),
        };
        assert!(client_for(&server).heartbeat("token", &pulse).await.unwrap_err().is_retryable());

        // And no server at all.
        let nowhere = KaslServer::new(&server_config("http://127.0.0.1:1", true)).unwrap();
        assert!(nowhere.heartbeat("token", &pulse).await.unwrap_err().is_retryable());
    }

    // === One pulse, and its record ===

    #[tokio::test]
    #[serial]
    async fn an_accepted_pulse_is_recorded_with_the_servers_cadence() {
        let _sandbox = sandbox();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(HEARTBEAT))
            .respond_with(accepted("paused"))
            .mount(&server)
            .await;

        let mut store = ServerPulse::new().unwrap();
        let (outcome, wait) = beat(&client_for(&server), "token", AgentState::Paused, DEFAULT_INTERVAL, &mut store)
            .await
            .unwrap();

        assert_eq!(outcome, Beat::Accepted(AgentState::Paused));
        assert_eq!(wait, std::time::Duration::from_secs(60));
        let record = store.last().unwrap().unwrap();
        assert_eq!(record.state, Some(AgentState::Paused));
        assert_eq!(record.stale_after_seconds, Some(180));
        assert_eq!(record.clock_skew_seconds, Some(2));
        assert_eq!(record.sent_at, Some(record.attempted_at));
        assert_eq!(record.next_at - record.attempted_at, Duration::seconds(60));
        assert!(record.error.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn a_failure_keeps_the_last_arrival_and_a_refusal_waits_longer() {
        let _sandbox = sandbox();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(HEARTBEAT))
            .respond_with(accepted("working"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(HEARTBEAT))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"error": "the token is not recognized"})))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let mut store = ServerPulse::new().unwrap();
        beat(&client, "token", AgentState::Working, DEFAULT_INTERVAL, &mut store).await.unwrap();
        let arrived = store.last().unwrap().unwrap().sent_at;

        let (outcome, wait) = beat(&client, "token", AgentState::Working, DEFAULT_INTERVAL, &mut store).await.unwrap();

        assert!(matches!(outcome, Beat::Refused(ref reason) if reason.contains("not recognized")), "{outcome:?}");
        assert_eq!(wait, REFUSED_RETRY);
        let record = store.last().unwrap().unwrap();
        // "When did one last get through" is the next question after a
        // failure, so the failure must not erase its answer.
        assert_eq!(record.sent_at, arrived);
        assert_eq!(record.state, Some(AgentState::Working));
        assert!(record.error.as_deref().unwrap().contains("not recognized"));
        assert_eq!(record.next_at - record.attempted_at, Duration::from_std(REFUSED_RETRY).unwrap());
    }

    // === The watcher's loop ===

    #[tokio::test]
    #[serial]
    async fn the_watcher_claims_what_it_sees_and_says_it_once_an_interval() {
        let _sandbox = sandbox();
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path(HEARTBEAT)).respond_with(Echo).mount(&server).await;
        connect(&server.uri(), true);

        let in_pause = Arc::new(AtomicBool::new(false));
        let mut pulser = pulser(&in_pause);

        // Before the day starts there is nothing to be working at.
        assert_eq!(pulser.tick().await.unwrap(), Some(Beat::Accepted(AgentState::Idle)));

        // The day starts: a changed claim goes at once, not a minute later.
        start_today();
        assert_eq!(pulser.tick().await.unwrap(), Some(Beat::Accepted(AgentState::Working)));

        // Nothing changed: nothing goes until the interval is up.
        assert_eq!(pulser.tick().await.unwrap(), None);
        assert_eq!(pulser.tick().await.unwrap(), None);

        // A break, seen by the monitor, goes at once too.
        in_pause.store(true, Ordering::Relaxed);
        assert_eq!(pulser.tick().await.unwrap(), Some(Beat::Accepted(AgentState::Paused)));

        // The day is closed: whatever the monitor says, it is over.
        in_pause.store(false, Ordering::Relaxed);
        Workdays::new().unwrap().insert_end(Local::now().date_naive()).unwrap();
        assert_eq!(pulser.tick().await.unwrap(), Some(Beat::Accepted(AgentState::Idle)));

        assert_eq!(claims(&server).await, vec!["idle", "working", "paused", "idle"]);
    }

    #[tokio::test]
    #[serial]
    async fn nothing_goes_while_the_pulse_is_off_or_there_is_no_connection() {
        let _sandbox = sandbox();
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path(HEARTBEAT)).respond_with(Echo).mount(&server).await;
        let in_pause = Arc::new(AtomicBool::new(false));
        let mut pulser = pulser(&in_pause);

        // Not connected at all.
        assert_eq!(pulser.tick().await.unwrap(), None);

        // Connected, which agrees to send days - not this.
        connect(&server.uri(), false);
        start_today();
        assert_eq!(pulser.tick().await.unwrap(), None);

        // On: it goes, and at once.
        connect(&server.uri(), true);
        assert_eq!(pulser.tick().await.unwrap(), Some(Beat::Accepted(AgentState::Working)));

        // Off again, mid-interval and with a change pending: nothing more.
        connect(&server.uri(), false);
        in_pause.store(true, Ordering::Relaxed);
        assert_eq!(pulser.tick().await.unwrap(), None);

        assert_eq!(claims(&server).await, vec!["working"]);
    }

    #[tokio::test]
    #[serial]
    async fn after_a_failure_the_watcher_does_not_ask_again_at_every_change() {
        let _sandbox = sandbox();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(HEARTBEAT))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        connect(&server.uri(), true);
        start_today();

        let in_pause = Arc::new(AtomicBool::new(false));
        let mut pulser = pulser(&in_pause);

        assert!(matches!(pulser.tick().await.unwrap(), Some(Beat::Deferred(_))));
        in_pause.store(true, Ordering::Relaxed);
        assert_eq!(pulser.tick().await.unwrap(), None);
        in_pause.store(false, Ordering::Relaxed);
        assert_eq!(pulser.tick().await.unwrap(), None);

        assert_eq!(
            claims(&server).await.len(),
            1,
            "a server that could not answer was asked again within the interval"
        );
    }

    #[tokio::test]
    #[serial]
    async fn a_missing_token_is_recorded_and_nothing_is_sent() {
        let _sandbox = sandbox();
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path(HEARTBEAT)).respond_with(Echo).mount(&server).await;
        connect(&server.uri(), true);

        let mut pulser = Pulser::with_token(Arc::new(AtomicBool::new(false)), Box::new(|| None));
        assert!(matches!(pulser.tick().await.unwrap(), Some(Beat::Refused(ref reason)) if reason.contains("kasl server connect")));

        assert!(claims(&server).await.is_empty());
        let record = ServerPulse::new().unwrap().last().unwrap().unwrap();
        assert!(record.error.unwrap().contains("kasl server connect"));
    }

    #[tokio::test]
    #[serial]
    async fn turning_it_on_again_asks_at_once_after_a_refusal() {
        // A person who fixed the cause of a refusal - the clock, the server -
        // turns the pulse on again to try. Waiting out five minutes would read
        // as the fix not working.
        let _sandbox = sandbox();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(HEARTBEAT))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({"error": "clock"})))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST")).and(path(HEARTBEAT)).respond_with(Echo).mount(&server).await;
        connect(&server.uri(), true);

        let mut pulser = pulser(&Arc::new(AtomicBool::new(false)));
        assert!(matches!(pulser.tick().await.unwrap(), Some(Beat::Refused(_))));
        assert_eq!(pulser.tick().await.unwrap(), None);

        // What `kasl server pulse enable` does to the record.
        ServerPulse::new().unwrap().clear().unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        assert_eq!(pulser.tick().await.unwrap(), Some(Beat::Accepted(AgentState::Idle)));
    }

    // === The commands ===

    fn kasl_cmd(dir: &Path) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kasl"));
        cmd.env("HOME", dir).env("LOCALAPPDATA", dir).stdin(Stdio::null());
        cmd
    }

    fn output_of(out: &Output) -> String {
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    }

    #[test]
    #[serial]
    fn the_pulse_reports_off_until_it_is_turned_on() {
        let dir = sandbox();
        connect("http://127.0.0.1:1", false);

        let out = kasl_cmd(dir.path()).args(["server", "pulse"]).output().unwrap();
        assert!(out.status.success(), "{}", output_of(&out));
        assert!(output_of(&out).contains("Pulse: off"), "{}", output_of(&out));
    }

    #[test]
    #[serial]
    fn the_pulse_reports_when_the_last_one_went() {
        let dir = sandbox();
        connect("http://127.0.0.1:1", true);
        let now = Utc::now() - Duration::seconds(42);
        ServerPulse::new()
            .unwrap()
            .record_accepted(now, now + Duration::seconds(60), AgentState::Working, 180, 0)
            .unwrap();

        let out = kasl_cmd(dir.path()).args(["server", "pulse"]).output().unwrap();
        let text = output_of(&out);
        assert!(out.status.success(), "{text}");
        assert!(text.contains("Pulse: on - the last one went 4"), "{text}");
        assert!(text.contains("(working)"), "{text}");
    }

    #[test]
    #[serial]
    fn status_reports_the_pulse_even_when_the_server_is_unreachable() {
        // The record is local, and an unreachable server is exactly when
        // "the last one went an hour ago" is worth reading.
        let dir = sandbox();
        connect("http://127.0.0.1:1", true);
        let then = Utc::now() - Duration::hours(1);
        ServerPulse::new()
            .unwrap()
            .record_failed(then, then + Duration::seconds(60), "cannot reach kasl-server")
            .unwrap();

        let out = kasl_cmd(dir.path()).args(["server", "status"]).output().unwrap();
        let text = output_of(&out);
        assert!(out.status.success(), "{text}");
        assert!(text.contains("did not arrive"), "{text}");
        // A record an hour old means nobody is sending, whatever it says.
        assert!(text.contains("kasl watch"), "{text}");
    }

    #[test]
    #[serial]
    fn disabling_forgets_the_consent_and_the_record_and_says_what_the_server_shows() {
        let dir = sandbox();
        connect("http://127.0.0.1:1", true);
        let now = Utc::now();
        ServerPulse::new()
            .unwrap()
            .record_accepted(now, now + Duration::seconds(60), AgentState::Working, 180, 0)
            .unwrap();

        let out = kasl_cmd(dir.path()).args(["server", "pulse", "disable"]).output().unwrap();
        let text = output_of(&out);
        assert!(out.status.success(), "{text}");
        // There is no "stop" to send; the server goes by silence. Promising a
        // blank dashboard would be a promise kasl cannot keep.
        assert!(text.contains("offline once that is 3 min old"), "{text}");

        assert!(!Config::read().unwrap().kasl_server.unwrap().pulse);
        assert!(ServerPulse::new().unwrap().last().unwrap().is_none());

        let again = kasl_cmd(dir.path()).args(["server", "pulse", "disable"]).output().unwrap();
        assert!(output_of(&again).contains("already off"), "{}", output_of(&again));
    }

    #[test]
    #[serial]
    fn enabling_without_a_connection_is_refused() {
        let dir = sandbox();

        let out = kasl_cmd(dir.path()).args(["server", "pulse", "enable"]).output().unwrap();
        assert!(!out.status.success(), "a pulse was turned on with nowhere to send it");
        assert!(output_of(&out).contains("not connected"), "{}", output_of(&out));
    }
}
