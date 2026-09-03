//! Deciding what a failure means for the queue.
//!
//! The outbox tests cover the bookkeeping; this covers the judgement on top of
//! it, which is where a queue goes wrong in the ways that cost data:
//!
//! * a day the server will never accept, queued anyway, retried forever;
//! * a day the server never answered about, dropped as though it had arrived;
//! * a backlog sent as one request the server refuses whole.
//!
//! Driven against a mock server rather than a fake client, so what is under
//! test includes reading the server's actual answer shape.

#[cfg(test)]
mod tests {
    use chrono::{Duration, Local, NaiveDate};
    use kasl::api::kasl_server::KaslServer;
    use kasl::api::kasl_server::UploadError;
    use kasl::db::server_outbox::ServerOutbox;
    use kasl::db::tasks::Tasks;
    use kasl::db::workdays::Workdays;
    use kasl::libs::config::KaslServerConfig;
    use kasl::libs::day_delivery::{BATCH_SIZE, Delivered, deliver, record_single};
    use kasl::libs::task::Task;
    use reqwest::StatusCode;
    use serial_test::serial;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Points the data directory at a fresh temporary one, and keeps it alive.
    ///
    /// Held by the caller rather than set up by an attribute because these
    /// tests are async, and the sandbox matters more here than the ceremony:
    /// without it a test run writes to - and reads - the real database on the
    /// developer's own machine.
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

    fn client_for(server: &MockServer) -> KaslServer {
        KaslServer::new(&KaslServerConfig {
            url: server.uri(),
            ca_certificate: None,
        })
        .unwrap()
    }

    /// Records a real workday for today, so there is a day to build and send.
    ///
    /// Today rather than a chosen date because `insert_start` stamps the row
    /// with the current time; what matters here is that a buildable day
    /// exists, not which date it falls on.
    fn record_a_workday() -> NaiveDate {
        let today = Local::now().date_naive();
        let mut workdays = Workdays::new().unwrap();
        workdays.insert_start(today).unwrap();

        let mut tasks = Tasks::new().unwrap();
        tasks.insert(&Task::new("Write the queue", "", Some(50))).unwrap();

        today
    }

    #[serial]
    #[tokio::test]
    async fn an_accepted_day_leaves_the_queue() {
        let _sandbox = sandbox();
        let date = record_a_workday();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/days/batch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accepted": 1,
                "rejected": 0,
                "results": [{
                    "status": "accepted",
                    "workday_id": "0f7b6f0e-4f2f-4a3e-9a2c-6a2b1c3d4e5f",
                    "date": date.to_string(),
                    "pauses": 0,
                    "tasks": 1,
                    "deleted_tasks": 0,
                    "privacy_level": "full"
                }]
            })))
            .mount(&server)
            .await;

        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(date, "offline").unwrap();

        let outcomes = deliver(&client_for(&server), "token", &mut outbox, &[date]).await.unwrap();

        assert!(matches!(outcomes[0], Delivered::Accepted { .. }), "got {:?}", outcomes[0]);
        assert_eq!(outbox.count().unwrap(), 0, "a delivered day stops being owed");
    }

    #[serial]
    #[tokio::test]
    async fn a_day_the_server_refuses_is_dropped_rather_than_retried_forever() {
        let _sandbox = sandbox();
        // The failure mode this exists to prevent: a day the server validated
        // and said no to, kept in the queue, resent on every flush, refused
        // every time, for the life of the installation.
        let date = record_a_workday();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/days/batch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accepted": 0,
                "rejected": 1,
                "results": [{
                    "status": "rejected",
                    "date": date.to_string(),
                    "error": "tasks[0]: name is empty"
                }]
            })))
            .mount(&server)
            .await;

        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(date, "offline").unwrap();

        let outcomes = deliver(&client_for(&server), "token", &mut outbox, &[date]).await.unwrap();

        match &outcomes[0] {
            Delivered::Refused { reason, .. } => assert!(reason.contains("name is empty"), "the reason should reach the user: {}", reason),
            other => panic!("a validated refusal is not worth retrying, got {:?}", other),
        }
        assert_eq!(outbox.count().unwrap(), 0, "it must not sit in the queue retrying");
    }

    #[serial]
    #[tokio::test]
    async fn a_server_that_could_not_answer_keeps_the_day() {
        let _sandbox = sandbox();
        let date = record_a_workday();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/days/batch"))
            .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({"error": "database is unavailable"})))
            .mount(&server)
            .await;

        let mut outbox = ServerOutbox::new().unwrap();

        let outcomes = deliver(&client_for(&server), "token", &mut outbox, &[date]).await.unwrap();

        assert!(matches!(outcomes[0], Delivered::Deferred { .. }), "got {:?}", outcomes[0]);
        assert_eq!(outbox.count().unwrap(), 1, "the day is still owed");
        assert_eq!(outbox.pending().unwrap()[0].date, date);
    }

    #[serial]
    #[tokio::test]
    async fn a_day_the_server_did_not_report_on_stays_owed() {
        let _sandbox = sandbox();
        // A truncated or partial answer. Dropping a day the server never
        // confirmed loses it for good; keeping one it did store costs a
        // harmless re-upload, so silence has to read as "still owed".
        let date = record_a_workday();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/days/batch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accepted": 0,
                "rejected": 0,
                "results": []
            })))
            .mount(&server)
            .await;

        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(date, "offline").unwrap();

        let outcomes = deliver(&client_for(&server), "token", &mut outbox, &[date]).await.unwrap();

        match &outcomes[0] {
            Delivered::Deferred { reason, .. } => assert!(reason.contains("did not report"), "unexpected reason: {}", reason),
            other => panic!("an unanswered day must stay owed, got {:?}", other),
        }
        assert_eq!(outbox.count().unwrap(), 1, "silence is not delivery");
    }

    #[serial]
    #[tokio::test]
    async fn a_date_whose_day_is_gone_stops_being_owed() {
        let _sandbox = sandbox();
        // The workday was deleted after the date was queued. There is nothing
        // left to send, so the debt goes with it rather than being retried
        // against a day that no longer exists.
        let server = MockServer::start().await;
        let missing = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();

        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(missing, "offline").unwrap();

        let outcomes = deliver(&client_for(&server), "token", &mut outbox, &[missing]).await.unwrap();

        assert!(outcomes.is_empty(), "there was nothing to report on");
        assert_eq!(outbox.count().unwrap(), 0, "a day that no longer exists is not owed");
    }

    #[serial]
    #[tokio::test]
    async fn nothing_is_sent_when_nothing_is_owed() {
        let _sandbox = sandbox();
        // Mounted with no mock at all: any request would answer 404 and show
        // up as a rejection, so a silent run is what proves none was made.
        let server = MockServer::start().await;
        let mut outbox = ServerOutbox::new().unwrap();

        let outcomes = deliver(&client_for(&server), "token", &mut outbox, &[]).await.unwrap();

        assert!(outcomes.is_empty());
        assert!(server.received_requests().await.unwrap().is_empty(), "an empty queue must not call the server");
    }

    #[serial]
    #[tokio::test]
    async fn a_backlog_is_one_request_rather_than_one_per_day() {
        let _sandbox = sandbox();
        // The reason the batch endpoint is used at all. A loop of single
        // uploads would pass every other test here and still be the thing
        // ADR 0005 exists to avoid.
        let date = record_a_workday();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/days/batch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accepted": 1,
                "rejected": 0,
                "results": [{
                    "status": "accepted",
                    "workday_id": "0f7b6f0e-4f2f-4a3e-9a2c-6a2b1c3d4e5f",
                    "date": date.to_string(),
                    "pauses": 0,
                    "tasks": 1,
                    "deleted_tasks": 0,
                    "privacy_level": "full"
                }]
            })))
            .mount(&server)
            .await;

        let mut outbox = ServerOutbox::new().unwrap();
        deliver(&client_for(&server), "token", &mut outbox, &[date]).await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "the backlog goes in one request");
        assert!(
            requests[0].url.path().ends_with("/days/batch"),
            "it goes to the batch endpoint, not the single-day one: {}",
            requests[0].url.path()
        );
    }

    #[serial]
    #[tokio::test]
    async fn a_batch_the_server_will_never_accept_does_not_queue_forever() {
        let _sandbox = sandbox();
        // A 4xx on the request itself - a malformed batch, a payload the
        // server will not take as sent. Queuing these builds a backlog that
        // retries on every flush and never empties, so they have to be
        // dropped and named instead.
        let date = record_a_workday();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/days/batch"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({"error": "days[0]: started_at has no offset"})))
            .mount(&server)
            .await;

        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(date, "offline").unwrap();

        let outcomes = deliver(&client_for(&server), "token", &mut outbox, &[date]).await.unwrap();

        match &outcomes[0] {
            Delivered::Refused { reason, .. } => assert!(reason.contains("no offset"), "the reason should reach the user: {}", reason),
            other => panic!("a 4xx will not come good on a retry, got {:?}", other),
        }
        assert_eq!(outbox.count().unwrap(), 0, "it must not sit in the queue retrying forever");
    }

    #[serial]
    #[tokio::test]
    async fn a_rate_limited_batch_keeps_its_days() {
        let _sandbox = sandbox();
        // 429 is inside the 4xx range and is the exception to it: the data is
        // fine, the timing is not. Sorting it by range alone would throw a
        // backlog away for sending too fast.
        let date = record_a_workday();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/days/batch"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({"error": "too many requests"})))
            .mount(&server)
            .await;

        let mut outbox = ServerOutbox::new().unwrap();

        let outcomes = deliver(&client_for(&server), "token", &mut outbox, &[date]).await.unwrap();

        assert!(
            matches!(outcomes[0], Delivered::Deferred { .. }),
            "a rate limit is a wait, got {:?}",
            outcomes[0]
        );
        assert_eq!(outbox.count().unwrap(), 1, "the day is kept for later");
    }

    #[serial]
    #[test]
    fn a_single_push_queues_only_what_is_worth_retrying() {
        let _sandbox = sandbox();
        // `push` and `flush` must agree about which failures are worth
        // keeping; the shared decision is the point of record_single, and
        // this is what holds the two paths together.
        let unreachable = UploadError::Retryable {
            message: "cannot reach kasl-server".to_string(),
        };
        let refused = UploadError::Rejected {
            status: StatusCode::BAD_REQUEST,
            message: "tasks[0]: name is empty".to_string(),
        };

        let mut outbox = ServerOutbox::new().unwrap();
        let waited = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        let never = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();

        let deferred = record_single(&mut outbox, waited, &unreachable).unwrap();
        let dropped = record_single(&mut outbox, never, &refused).unwrap();

        assert!(matches!(deferred, Delivered::Deferred { .. }), "got {:?}", deferred);
        assert!(matches!(dropped, Delivered::Refused { .. }), "got {:?}", dropped);

        let owed: Vec<NaiveDate> = outbox.pending().unwrap().into_iter().map(|day| day.date).collect();
        assert_eq!(owed, vec![waited], "only the day worth retrying is owed");
    }

    #[serial]
    #[tokio::test]
    async fn a_backlog_longer_than_a_batch_is_split_into_several() {
        let _sandbox = sandbox();
        // The server caps a batch and refuses a longer one whole with 413, so
        // a year of backfill has to be split here. Asserting the constant
        // against a literal would only restate its definition; what matters is
        // that more days than fit produce more than one request.
        let days = BATCH_SIZE + 5;
        let first = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let dates: Vec<NaiveDate> = (0..days as i64).map(|offset| first + Duration::days(offset)).collect();

        let mut workdays = Workdays::new().unwrap();
        for date in &dates {
            workdays.insert_start(*date).unwrap();
            workdays.update_start(*date, date.and_hms_opt(9, 0, 0).unwrap()).unwrap();
        }

        let server = MockServer::start().await;
        // Answering with an empty result list leaves every day owed, which is
        // fine here: what is counted is the requests, not the outcomes.
        Mock::given(method("POST"))
            .and(path("/api/v1/days/batch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "accepted": 0, "rejected": 0, "results": []
            })))
            .mount(&server)
            .await;

        let mut outbox = ServerOutbox::new().unwrap();
        deliver(&client_for(&server), "token", &mut outbox, &dates).await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2, "{days} days should not go as one oversized request");

        // And every day is in exactly one of them: a split that drops the
        // remainder would also produce two requests.
        let sent: usize = requests
            .iter()
            .map(|request| {
                serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["days"]
                    .as_array()
                    .expect("a batch carries a days array")
                    .len()
            })
            .sum();
        assert_eq!(sent, days, "every day should be sent exactly once");
    }
}
