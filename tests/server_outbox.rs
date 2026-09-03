//! The outbox of days still owed to kasl-server.
//!
//! What is under test is the bookkeeping a week offline depends on: that a day
//! is owed once however many times it fails, that the debt survives being
//! retried, and that a day the server will never accept does not become a row
//! which retries forever.

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use kasl::db::server_outbox::ServerOutbox;
    use serial_test::serial;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    struct OutboxTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for OutboxTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("HOME", temp_dir.path());
            }
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("LOCALAPPDATA", temp_dir.path());
            }
            OutboxTestContext { _temp_dir: temp_dir }
        }
    }

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test_context(OutboxTestContext)]
    #[serial]
    #[test]
    fn a_queued_day_is_listed_with_why_it_is_there(_ctx: &mut OutboxTestContext) {
        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(date("2026-08-31"), "cannot reach kasl-server").unwrap();

        let owed = outbox.pending().unwrap();

        assert_eq!(owed.len(), 1);
        assert_eq!(owed[0].date, date("2026-08-31"));
        assert_eq!(owed[0].attempts, 1);
        assert_eq!(
            owed[0].last_error.as_deref(),
            Some("cannot reach kasl-server"),
            "the reason is the only answer to 'why is this still here'"
        );
    }

    #[test_context(OutboxTestContext)]
    #[serial]
    #[test]
    fn failing_the_same_day_twice_owes_it_once(_ctx: &mut OutboxTestContext) {
        // The failure this guards is a fortnight offline becoming a fortnight
        // of duplicate rows for one date, and then a fortnight of duplicate
        // uploads of the same day.
        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(date("2026-08-31"), "cannot reach kasl-server").unwrap();
        outbox.enqueue(date("2026-08-31"), "cannot reach kasl-server").unwrap();
        outbox.enqueue(date("2026-08-31"), "still nothing").unwrap();

        let owed = outbox.pending().unwrap();

        assert_eq!(owed.len(), 1, "one date is one debt");
        assert_eq!(owed[0].attempts, 3, "but the attempts are counted");
        assert_eq!(owed[0].last_error.as_deref(), Some("still nothing"), "the latest reason wins");
    }

    #[test_context(OutboxTestContext)]
    #[serial]
    #[test]
    fn retrying_does_not_reset_how_long_a_day_has_been_stuck(_ctx: &mut OutboxTestContext) {
        // `queued_at` answers "how long has this been failing", which a
        // refresh on every attempt would quietly erase - and a day stuck for a
        // month would look like it arrived a minute ago.
        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(date("2026-08-31"), "first failure").unwrap();
        let first = outbox.pending().unwrap()[0].queued_at;

        outbox.enqueue(date("2026-08-31"), "second failure").unwrap();
        let after_retry = outbox.pending().unwrap()[0].clone();

        assert_eq!(after_retry.queued_at, first, "the age of the debt survives a retry");
        assert!(after_retry.last_attempt_at.is_some(), "but the last attempt is recorded");
    }

    #[test_context(OutboxTestContext)]
    #[serial]
    #[test]
    fn days_come_back_oldest_first(_ctx: &mut OutboxTestContext) {
        // A backlog is delivered in the order it happened; a dashboard filling
        // in at random reads as a machine malfunctioning.
        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(date("2026-08-31"), "offline").unwrap();
        outbox.enqueue(date("2026-08-29"), "offline").unwrap();
        outbox.enqueue(date("2026-08-30"), "offline").unwrap();

        let owed: Vec<NaiveDate> = outbox.pending().unwrap().into_iter().map(|day| day.date).collect();

        assert_eq!(owed, vec![date("2026-08-29"), date("2026-08-30"), date("2026-08-31")]);
    }

    #[test_context(OutboxTestContext)]
    #[serial]
    #[test]
    fn a_delivered_day_stops_being_owed(_ctx: &mut OutboxTestContext) {
        let mut outbox = ServerOutbox::new().unwrap();
        outbox.enqueue(date("2026-08-31"), "offline").unwrap();

        assert!(outbox.remove(date("2026-08-31")).unwrap(), "the row was there to remove");
        assert_eq!(outbox.count().unwrap(), 0);
        assert!(outbox.pending().unwrap().is_empty());
    }

    #[test_context(OutboxTestContext)]
    #[serial]
    #[test]
    fn removing_a_day_that_was_never_owed_says_so(_ctx: &mut OutboxTestContext) {
        // The return value is what tells a caller whether anything was
        // cancelled; a bare Ok would make a no-op indistinguishable from a
        // delivery.
        let mut outbox = ServerOutbox::new().unwrap();

        assert!(!outbox.remove(date("2026-08-31")).unwrap(), "nothing was owed for that date");
    }

    #[test_context(OutboxTestContext)]
    #[serial]
    #[test]
    fn an_empty_outbox_owes_nothing(_ctx: &mut OutboxTestContext) {
        let outbox = ServerOutbox::new().unwrap();

        assert_eq!(outbox.count().unwrap(), 0);
        assert!(outbox.pending().unwrap().is_empty());
    }

    #[test_context(OutboxTestContext)]
    #[serial]
    #[test]
    fn the_queue_survives_being_reopened(_ctx: &mut OutboxTestContext) {
        // The whole point of a queue is that it outlives the process that
        // failed to send. An in-memory one would pass every test above and
        // lose the backlog on exit.
        {
            let mut outbox = ServerOutbox::new().unwrap();
            outbox.enqueue(date("2026-08-31"), "offline").unwrap();
        }

        let reopened = ServerOutbox::new().unwrap();

        assert_eq!(reopened.count().unwrap(), 1, "the debt outlives the process");
        assert_eq!(reopened.pending().unwrap()[0].date, date("2026-08-31"));
    }
}
