//! Cutting and ordering the inbox.
//!
//! Each cut has a test that holds an issue *out*: a filter that keeps
//! everything is indistinguishable from no filter, and a green test that
//! only checks what stays would not notice.

use chrono::{Duration, Local, NaiveDateTime};
use kasl::db::jira_inbox::JiraInboxItem;
use kasl::libs::inbox_filter::{InboxFilter, InboxSort, PriorityCut, explain, parse_window, priority_cut_named, sort};

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

fn item(key: &str) -> JiraInboxItem {
    JiraInboxItem {
        issue_key: key.to_string(),
        issue_id: "1".to_string(),
        summary: format!("Summary for {key}"),
        status_id: Some("10".to_string()),
        status_name: "Open".to_string(),
        priority: Some("Medium".to_string()),
        priority_rank: 3,
        sort_value: Some(5.0),
        url: String::new(),
        first_seen: now() - Duration::days(30),
        last_seen: now(),
        notified: true,
        pinned: false,
        dismissed: false,
        raw_updated: None,
        gone_at: None,
        last_change: None,
        changed_at: None,
        taken_at: None,
        snoozed_until: None,
        woke_at: None,
    }
}

fn keys(items: &[JiraInboxItem]) -> Vec<&str> {
    items.iter().map(|i| i.issue_key.as_str()).collect()
}

#[test]
fn windows_are_a_number_and_one_letter() {
    assert_eq!(parse_window("7d").unwrap(), Duration::days(7));
    assert_eq!(parse_window("12h").unwrap(), Duration::hours(12));
    assert_eq!(parse_window("2w").unwrap(), Duration::weeks(2));
    assert_eq!(parse_window("3").unwrap(), Duration::days(3), "a bare number is days");
    for bad in ["0d", "-1d", "d", "7m", "week", ""] {
        assert!(parse_window(bad).is_err(), "'{bad}' was accepted as a window");
    }
}

#[test]
fn an_empty_filter_keeps_the_whole_inbox() {
    let filter = InboxFilter::default();
    assert!(filter.is_empty());
    let items = vec![item("KA-1"), item("KA-2")];
    assert_eq!(filter.apply(items, now()).len(), 2);
}

#[test]
fn since_keeps_what_arrived_inside_the_window() {
    let mut fresh = item("KA-fresh");
    fresh.first_seen = now() - Duration::hours(20);
    let stale = item("KA-stale");
    let filter = InboxFilter {
        since: Some(Duration::days(1)),
        ..Default::default()
    };
    assert_eq!(keys(&filter.apply(vec![stale, fresh], now())), vec!["KA-fresh"]);
}

#[test]
fn changed_keeps_what_changed_inside_the_window_and_drops_the_never_changed() {
    let mut recent = item("KA-recent");
    recent.changed_at = Some(now() - Duration::days(2));
    let mut old = item("KA-old");
    old.changed_at = Some(now() - Duration::days(20));
    let never = item("KA-never");
    let filter = InboxFilter {
        changed: Some(Duration::days(7)),
        ..Default::default()
    };
    assert_eq!(keys(&filter.apply(vec![old, never, recent], now())), vec!["KA-recent"]);
}

#[test]
fn min_score_drops_low_scores_and_issues_without_one() {
    let mut high = item("KA-high");
    high.sort_value = Some(8.0);
    let mut edge = item("KA-edge");
    edge.sort_value = Some(5.0);
    let mut low = item("KA-low");
    low.sort_value = Some(4.9);
    let mut none = item("KA-none");
    none.sort_value = None;
    let filter = InboxFilter {
        min_score: Some(5.0),
        ..Default::default()
    };
    assert_eq!(keys(&filter.apply(vec![low, none, high, edge], now())), vec!["KA-high", "KA-edge"]);
}

#[test]
fn priority_names_come_from_the_inbox_and_plus_widens_upwards() {
    let mut highest = item("KA-highest");
    highest.priority = Some("Highest".to_string());
    highest.priority_rank = 1;
    let mut high = item("KA-high");
    high.priority = Some("High".to_string());
    high.priority_rank = 2;
    let medium = item("KA-medium");
    let mut bare = item("KA-bare");
    bare.priority = None;
    bare.priority_rank = 999;
    let items = vec![highest, high, medium, bare];

    assert_eq!(
        priority_cut_named(&items, "high").unwrap(),
        PriorityCut::Exactly(2),
        "names match without regard to case"
    );
    assert_eq!(priority_cut_named(&items, "High+").unwrap(), PriorityCut::AtLeast(2));
    let err = priority_cut_named(&items, "Urgent").unwrap_err().to_string();
    assert!(err.contains("High") && err.contains("Medium"), "the error names the known priorities: {err}");

    let exact = InboxFilter {
        priority: Some(PriorityCut::Exactly(2)),
        ..Default::default()
    };
    assert_eq!(keys(&exact.apply(items.clone(), now())), vec!["KA-high"]);
    let and_above = InboxFilter {
        priority: Some(PriorityCut::AtLeast(2)),
        ..Default::default()
    };
    assert_eq!(keys(&and_above.apply(items, now())), vec!["KA-highest", "KA-high"]);
}

#[test]
fn status_matches_the_name_or_the_id_without_regard_to_case() {
    let open = item("KA-open");
    let mut progress = item("KA-progress");
    progress.status_id = Some("3".to_string());
    progress.status_name = "In Progress".to_string();
    let by_name = InboxFilter {
        status: Some("in progress".to_string()),
        ..Default::default()
    };
    assert_eq!(keys(&by_name.apply(vec![open.clone(), progress.clone()], now())), vec!["KA-progress"]);
    let by_id = InboxFilter {
        status: Some("10".to_string()),
        ..Default::default()
    };
    assert_eq!(keys(&by_id.apply(vec![open, progress], now())), vec!["KA-open"]);
}

#[test]
fn cuts_combine_with_and() {
    let mut fresh_low = item("KA-fresh-low");
    fresh_low.first_seen = now() - Duration::hours(1);
    fresh_low.sort_value = Some(1.0);
    let mut fresh_high = item("KA-fresh-high");
    fresh_high.first_seen = now() - Duration::hours(1);
    fresh_high.sort_value = Some(9.0);
    let mut stale_high = item("KA-stale-high");
    stale_high.sort_value = Some(9.0);
    let filter = InboxFilter {
        since: Some(Duration::days(1)),
        min_score: Some(5.0),
        ..Default::default()
    };
    assert_eq!(keys(&filter.apply(vec![fresh_low, stale_high, fresh_high], now())), vec!["KA-fresh-high"]);
}

#[test]
fn sorting_keeps_pinned_first_and_gone_last_whatever_the_key() {
    let mut pinned_old = item("KA-pinned");
    pinned_old.pinned = true;
    pinned_old.first_seen = now() - Duration::days(90);
    pinned_old.priority_rank = 5;
    let mut gone_urgent = item("KA-gone");
    gone_urgent.gone_at = Some(now());
    gone_urgent.priority_rank = 1;
    gone_urgent.first_seen = now();
    let mut newest = item("KA-newest");
    newest.first_seen = now() - Duration::hours(1);
    let mut urgent = item("KA-urgent");
    urgent.priority_rank = 2;
    urgent.first_seen = now() - Duration::days(10);

    let mut by_new = vec![gone_urgent.clone(), urgent.clone(), newest.clone(), pinned_old.clone()];
    sort(&mut by_new, InboxSort::New);
    assert_eq!(keys(&by_new), vec!["KA-pinned", "KA-newest", "KA-urgent", "KA-gone"]);

    let mut by_priority = vec![gone_urgent, newest, urgent, pinned_old];
    sort(&mut by_priority, InboxSort::Priority);
    assert_eq!(keys(&by_priority), vec!["KA-pinned", "KA-urgent", "KA-newest", "KA-gone"]);
}

#[test]
fn sorting_by_change_puts_the_never_changed_last() {
    let mut recent = item("KA-recent");
    recent.changed_at = Some(now() - Duration::hours(1));
    let mut older = item("KA-older");
    older.changed_at = Some(now() - Duration::days(3));
    let never = item("KA-never");
    let mut items = vec![never, older, recent];
    sort(&mut items, InboxSort::Changed);
    assert_eq!(keys(&items), vec!["KA-recent", "KA-older", "KA-never"]);
}

#[test]
fn the_score_order_is_the_inbox_order_it_came_in() {
    let mut items = vec![item("KA-1"), item("KA-2"), item("KA-3")];
    sort(&mut items, InboxSort::Score);
    assert_eq!(keys(&items), vec!["KA-1", "KA-2", "KA-3"]);
}

#[test]
fn why_names_the_field_the_score_came_from() {
    // The number in the SCORE column is meaningless without the name of the
    // Jira field it was read from - that name is most of the answer, and it
    // lives in config rather than on the row.
    let item = item("KA-1");
    let reasons = explain(&item, Some("Scoring"), now());

    let score = reasons.iter().find(|r| r.what == "score").expect("score is always explained");
    assert_eq!(score.value, "5");
    assert!(score.because.contains("Scoring"), "the answer must name the field: {}", score.because);

    // With no configured field there is still an honest answer, just a vaguer
    // one - never a number presented as if it explained itself.
    let unnamed = explain(&item, None, now());
    let score = unnamed.iter().find(|r| r.what == "score").unwrap();
    assert!(score.because.contains("ranking field"));
}

#[test]
fn why_explains_the_order_rather_than_inventing_a_formula() {
    // kasl does not compute importance: the score is Jira's field and the rank
    // is Jira's priority id. A decomposed local score ("High +3, due tomorrow
    // +2") would be a second answer disagreeing with Jira's, so the explanation
    // must stay about the order and never add points up.
    let mut item = item("KA-2");
    item.priority = Some("High".to_string());
    item.priority_rank = 2;
    item.pinned = true;

    let reasons = explain(&item, Some("Scoring"), now());
    let whats: Vec<&str> = reasons.iter().map(|r| r.what).collect();
    assert!(whats.contains(&"score") && whats.contains(&"priority") && whats.contains(&"pinned"));

    let priority = reasons.iter().find(|r| r.what == "priority").unwrap();
    assert_eq!(priority.value, "High");
    assert!(priority.because.contains("2"), "the rank is Jira's priority id, and saying so is the point");

    // Nothing anywhere claims a score was added up from parts.
    for reason in &reasons {
        assert!(!reason.because.contains('+'), "no reason may read as a term in a formula: {}", reason.because);
    }
}

#[test]
fn why_says_what_each_mark_does_to_the_list() {
    // The marks are exactly the things that move a row somewhere the score
    // does not explain, so each one has to say what it did.
    let mut item = item("KA-3");
    let now = now();
    item.snoozed_until = Some(now + Duration::days(2));
    item.taken_at = Some(now - Duration::hours(5));
    item.last_change = Some("status→In Progress".to_string());
    item.changed_at = Some(now - Duration::hours(1));

    let reasons = explain(&item, Some("Scoring"), now);
    let whats: Vec<&str> = reasons.iter().map(|r| r.what).collect();
    assert!(whats.contains(&"snoozed"), "a sleeping issue must say it is asleep");
    assert!(whats.contains(&"taken"));
    assert!(whats.contains(&"changed"));

    // A snooze that has already run out is not a reason for anything.
    item.snoozed_until = Some(now - Duration::minutes(1));
    let reasons = explain(&item, Some("Scoring"), now);
    assert!(!reasons.iter().any(|r| r.what == "snoozed"), "a snooze that ran out no longer explains the row");
}

#[test]
fn why_is_honest_about_a_missing_score() {
    // An issue with no value in the ranking field is the case where the user
    // most needs the explanation: it sits at the bottom for a reason that is
    // invisible in the list.
    let mut item = item("KA-4");
    item.sort_value = None;
    item.priority = None;

    let reasons = explain(&item, Some("Scoring"), now());
    let score = reasons.iter().find(|r| r.what == "score").unwrap();
    assert_eq!(score.value, "—");
    assert!(score.because.contains("empty"), "an empty field must be named as empty: {}", score.because);

    let priority = reasons.iter().find(|r| r.what == "priority").unwrap();
    assert!(priority.because.contains("no priority"), "{}", priority.because);
}
