//! Tests for toast buttons: the mailbox, the key guard, and what a press does.
//!
//! The mailbox is the seam a button press crosses, so the tests are about the
//! things that go wrong at a seam: a line that cannot be parsed, a drain that
//! races a press, a key that is not a key.

use chrono::Duration;
use kasl::db::jira_inbox::{JiraInbox, JiraInboxUpsert};
use kasl::libs::toast_action::{Mailbox, ToastAction, ToastRequest, validate_issue_key};
use kasl::libs::toast_apply::{self, Applied};
use serial_test::serial;
use tempfile::TempDir;
use test_context::{TestContext, test_context};

struct ToastContext {
    _temp_dir: TempDir,
}

impl TestContext for ToastContext {
    fn setup() -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        // SAFETY: every test in this file is #[serial], so nothing else is
        // reading these variables while they change.
        unsafe {
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
        }
        ToastContext { _temp_dir: temp_dir }
    }
}

fn upsert(key: &str) -> JiraInboxUpsert {
    JiraInboxUpsert {
        issue_key: key.to_string(),
        issue_id: "1".to_string(),
        summary: format!("Summary for {key}"),
        status_id: Some("10".to_string()),
        status_name: "Open".to_string(),
        priority: Some("High".to_string()),
        priority_rank: 2,
        sort_value: None,
        url: format!("https://example.invalid/browse/{key}"),
        raw_updated: None,
    }
}

#[test_context(ToastContext)]
#[test]
#[serial]
fn a_posted_request_comes_back_out(_ctx: &mut ToastContext) {
    let mailbox = Mailbox::open().unwrap();
    mailbox.post(&ToastRequest::new(ToastAction::Take, "KA-1")).unwrap();
    mailbox.post(&ToastRequest::new(ToastAction::Snooze, "KA-2")).unwrap();

    let collected = mailbox.collect().unwrap();
    assert_eq!(
        collected,
        vec![ToastRequest::new(ToastAction::Take, "KA-1"), ToastRequest::new(ToastAction::Snooze, "KA-2")],
        "the mailbox must hand back what was posted, in order"
    );
}

#[test_context(ToastContext)]
#[test]
#[serial]
fn a_drained_mailbox_is_empty(_ctx: &mut ToastContext) {
    let mailbox = Mailbox::open().unwrap();
    mailbox.post(&ToastRequest::new(ToastAction::Dismiss, "KA-3")).unwrap();

    assert_eq!(mailbox.collect().unwrap().len(), 1);
    assert!(
        mailbox.collect().unwrap().is_empty(),
        "a second drain must be empty, or one press would be carried out over and over"
    );
}

#[test_context(ToastContext)]
#[test]
#[serial]
fn an_empty_mailbox_is_not_an_error(_ctx: &mut ToastContext) {
    // The common case: the daemon looks every two seconds and finds nothing.
    assert!(Mailbox::open().unwrap().collect().unwrap().is_empty());
}

#[test_context(ToastContext)]
#[test]
#[serial]
fn a_broken_line_does_not_block_the_ones_behind_it(_ctx: &mut ToastContext) {
    let mailbox = Mailbox::open().unwrap();
    mailbox.post(&ToastRequest::new(ToastAction::Take, "KA-4")).unwrap();
    // Something that is not a request at all, as a half-written line or a
    // build with a different wire format would leave behind.
    std::fs::write(mailbox.path(), "take\tKA-4\nnonsense\nfly\tKA-5\nsnooze\t\n\ndismiss\tKA-6\n").unwrap();

    let collected = mailbox.collect().unwrap();
    assert_eq!(
        collected,
        vec![ToastRequest::new(ToastAction::Take, "KA-4"), ToastRequest::new(ToastAction::Dismiss, "KA-6")],
        "unparseable lines are skipped; a press behind one must still be carried out"
    );
}

#[test]
fn every_action_survives_a_round_trip_through_its_wire_name() {
    for action in ToastAction::ALL {
        assert_eq!(
            ToastAction::parse(action.as_str()),
            Some(action),
            "{:?} must parse back from its own wire name",
            action
        );
    }
    assert_eq!(ToastAction::parse("open"), None, "open is not a decision, so it must not parse as one");
    assert_eq!(ToastAction::parse(""), None);
}

#[test]
fn a_key_that_could_escape_a_shortcut_is_refused() {
    // These go into a file name and onto a command line, so the guard is the
    // thing standing between Jira's data and the shell.
    for bad in [
        "",
        "KA 1",             // a space would split the command line
        "KA-1\" && calc",   // quote-and-chain
        "KA-1;calc",        // separator
        "..\\..\\evil",     // path traversal out of the shortcut dir
        "KA/1",             // path separator
        "KA-1\ttake",       // a tab would forge a second mailbox field
        "KA-1\ntake\tKA-2", // a newline would forge a whole second line
    ] {
        assert!(validate_issue_key(bad).is_err(), "{bad:?} must be refused as an issue key");
    }
    for good in ["KA-1", "PROJ-12345", "ABC_DEF-7", "A1"] {
        assert!(validate_issue_key(good).is_ok(), "{good:?} is the shape Jira uses and must be accepted");
    }
    assert!(validate_issue_key(&"A".repeat(65)).is_err(), "an absurdly long key must be refused");
}

#[test_context(ToastContext)]
#[test]
#[serial]
fn take_from_a_toast_makes_the_same_task_the_cli_would(_ctx: &mut ToastContext) {
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-10")]).unwrap();

    let outcome = toast_apply::apply(&ToastRequest::new(ToastAction::Take, "KA-10")).unwrap();
    assert_eq!(outcome, Applied::Taken);

    let item = db.get_by_key("KA-10").unwrap().unwrap();
    assert!(item.taken_at.is_some(), "the issue must wear the taken mark, as `inbox take` leaves it");

    // The second press is the interesting one: a toast can be pressed again
    // from the notification centre long after it appeared.
    let again = toast_apply::apply(&ToastRequest::new(ToastAction::Take, "KA-10")).unwrap();
    assert!(
        matches!(again, Applied::AlreadyTaken(_)),
        "a second press must report the existing task, not make a duplicate one, got {again:?}"
    );
}

#[test_context(ToastContext)]
#[test]
#[serial]
fn snooze_from_a_toast_puts_the_issue_to_sleep(_ctx: &mut ToastContext) {
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-11")]).unwrap();

    let before = chrono::Local::now().naive_local();
    let outcome = toast_apply::snooze("KA-11", Duration::days(3)).unwrap();
    let Applied::Snoozed(until) = outcome else {
        panic!("expected a snooze, got {outcome:?}");
    };
    assert!(
        until > before + Duration::days(2),
        "a three-day snooze must land about three days out, got {until}"
    );

    let item = db.get_by_key("KA-11").unwrap().unwrap();
    assert!(item.snoozed_until.is_some(), "the row must record when it is due back");
    // A sleeping issue is out of the awake list, which is the whole point.
    let awake = db.list_active_at(false, false).unwrap();
    assert!(!awake.iter().any(|i| i.issue_key == "KA-11"), "a snoozed issue must leave the awake list");
}

#[test_context(ToastContext)]
#[test]
#[serial]
fn dismiss_from_a_toast_hides_the_issue(_ctx: &mut ToastContext) {
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-12")]).unwrap();

    assert_eq!(
        toast_apply::apply(&ToastRequest::new(ToastAction::Dismiss, "KA-12")).unwrap(),
        Applied::Dismissed
    );
    let awake = db.list_active_at(false, false).unwrap();
    assert!(!awake.iter().any(|i| i.issue_key == "KA-12"), "a dismissed issue must leave the list");
}

#[test_context(ToastContext)]
#[test]
#[serial]
fn a_press_on_an_issue_that_is_gone_says_so_instead_of_failing(_ctx: &mut ToastContext) {
    // The race the mailbox makes possible: the toast is on screen, the issue
    // is synced away, then the button is pressed.
    let _ = JiraInbox::new().unwrap();
    for action in ToastAction::ALL {
        let outcome = toast_apply::apply(&ToastRequest::new(action, "KA-404")).unwrap();
        assert_eq!(outcome, Applied::NotFound, "{:?} on a missing key must report it, not error", action);
        assert!(!outcome.settled(), "a missing issue is not settled, so its buttons must stay");
    }
}

#[test_context(ToastContext)]
#[test]
#[serial]
fn the_drain_carries_out_everything_waiting(_ctx: &mut ToastContext) {
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-20"), upsert("KA-21"), upsert("KA-22")]).unwrap();

    let mailbox = Mailbox::open().unwrap();
    mailbox.post(&ToastRequest::new(ToastAction::Take, "KA-20")).unwrap();
    mailbox.post(&ToastRequest::new(ToastAction::Dismiss, "KA-21")).unwrap();
    // A missing key in the middle: the ones behind it must still happen.
    mailbox.post(&ToastRequest::new(ToastAction::Take, "KA-404")).unwrap();
    mailbox.post(&ToastRequest::new(ToastAction::Snooze, "KA-22")).unwrap();

    let done = kasl::libs::jira_inbox::drain_toast_actions().unwrap();
    assert_eq!(done, 4, "all four presses are decisions the user already made; each must be answered");

    assert!(db.get_by_key("KA-20").unwrap().unwrap().taken_at.is_some());
    assert!(db.get_by_key("KA-21").unwrap().unwrap().dismissed);
    assert!(db.get_by_key("KA-22").unwrap().unwrap().snoozed_until.is_some());
    assert!(
        mailbox.collect().unwrap().is_empty(),
        "the drain must empty the mailbox, or every press would repeat on the next look"
    );
}

#[test]
fn every_outcome_says_what_happened_and_names_the_issue() {
    // A toast answering a press is the only feedback the user gets, so an
    // empty or key-less summary would leave them unsure it registered.
    let outcomes = [
        Applied::Taken,
        Applied::AlreadyTaken("KA-1 something".into()),
        Applied::Snoozed(chrono::Local::now().naive_local()),
        Applied::Dismissed,
        Applied::NotFound,
    ];
    for outcome in outcomes {
        let summary = outcome.summary("KA-1");
        assert!(summary.contains("KA-1"), "{outcome:?} must name the issue it is about, got {summary:?}");
        assert!(summary.len() > "KA-1".len() + 3, "{outcome:?} must say what happened, got {summary:?}");
    }
}

#[test]
fn the_three_buttons_are_the_three_triage_decisions() {
    // A fourth button would make a toast a menu, and a menu asks to be read
    // rather than glanced at. If this list grows, that was a decision.
    assert_eq!(ToastAction::ALL.len(), 3);
    assert_eq!(ToastAction::ALL[0], ToastAction::Take, "Take leads: it is the one that moves work forward");
    assert_eq!(
        ToastAction::ALL[2],
        ToastAction::Dismiss,
        "Dismiss is last: it is the only one waiting cannot undo"
    );
    let labels: Vec<&str> = ToastAction::ALL.iter().map(|a| a.label()).collect();
    assert_eq!(labels, vec!["Take", "Snooze", "Dismiss"]);
}
