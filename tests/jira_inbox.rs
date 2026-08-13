//! Tests for Jira inbox reconciliation and change tracking.
//!
//! Covers the sync freeze fix: issues missing from a poll get `gone_at`,
//! reappearing issues come back, and visible changes (status, priority,
//! score) are detected and described.

use chrono::{Duration, Local};
use kasl::db::jira_inbox::{JiraInbox, JiraInboxItem, JiraInboxUpsert};
use serial_test::serial;
use tempfile::TempDir;
use test_context::{TestContext, test_context};

struct InboxTestContext {
    _temp_dir: TempDir,
}

impl TestContext for InboxTestContext {
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
        InboxTestContext { _temp_dir: temp_dir }
    }
}

fn upsert(key: &str) -> JiraInboxUpsert {
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

#[test_context(InboxTestContext)]
#[serial]
#[test]
fn test_upsert_detects_new_and_unchanged(_ctx: &mut InboxTestContext) {
    let db = JiraInbox::new().unwrap();

    let result = db.upsert_batch(&[upsert("KA-1")]).unwrap();
    assert_eq!(result.new_keys, vec!["KA-1"]);
    assert!(result.changed.is_empty());

    // Same payload again: no new keys, no changes.
    let result = db.upsert_batch(&[upsert("KA-1")]).unwrap();
    assert!(result.new_keys.is_empty());
    assert_eq!(result.updated, 1);
    assert!(result.changed.is_empty());
}

#[test_context(InboxTestContext)]
#[serial]
#[test]
fn test_upsert_describes_visible_changes(_ctx: &mut InboxTestContext) {
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-1")]).unwrap();

    // Status, priority up, and score all change at once.
    let mut changed = upsert("KA-1");
    changed.status_id = Some("20".to_string());
    changed.status_name = "In Progress".to_string();
    changed.priority = Some("High".to_string());
    changed.priority_rank = 2;
    changed.sort_value = Some(8.0);

    let result = db.upsert_batch(&[changed]).unwrap();
    assert_eq!(result.changed.len(), 1);
    let change = &result.changed[0].change;
    assert!(change.contains("status→In Progress"), "change was: {change}");
    assert!(change.contains("↑prio High"), "change was: {change}");
    assert!(change.contains("score 5→8"), "change was: {change}");

    let item = db.get_by_key("KA-1").unwrap().unwrap();
    assert_eq!(item.last_change.as_deref(), Some(change.as_str()));
    assert!(item.changed_at.is_some());
}

#[test_context(InboxTestContext)]
#[serial]
#[test]
fn test_priority_down_is_described(_ctx: &mut InboxTestContext) {
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-1")]).unwrap();

    let mut lowered = upsert("KA-1");
    lowered.priority = Some("Low".to_string());
    lowered.priority_rank = 4;

    let result = db.upsert_batch(&[lowered]).unwrap();
    assert_eq!(result.changed.len(), 1);
    assert!(result.changed[0].change.contains("↓prio Low"));
}

#[test_context(InboxTestContext)]
#[serial]
#[test]
fn test_mark_gone_reconciles_missing_issues(_ctx: &mut InboxTestContext) {
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-1"), upsert("KA-2")]).unwrap();

    // Next poll returns only KA-1: KA-2 must be marked gone.
    let gone = db.mark_gone(&["KA-1".to_string()]).unwrap();
    assert_eq!(gone, vec!["KA-2"]);

    // Default list hides gone issues; --all shows them at the bottom.
    let visible = db.list_active(false).unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].issue_key, "KA-1");

    let all = db.list_active(true).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].issue_key, "KA-2");
    assert!(all[1].gone_at.is_some());

    // A second reconcile with the same poll does not report KA-2 again.
    let gone = db.mark_gone(&["KA-1".to_string()]).unwrap();
    assert!(gone.is_empty());
}

#[test_context(InboxTestContext)]
#[serial]
#[test]
fn test_gone_issue_comes_back(_ctx: &mut InboxTestContext) {
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-1")]).unwrap();
    db.mark_gone(&[]).unwrap();
    assert!(db.get_by_key("KA-1").unwrap().unwrap().gone_at.is_some());

    // The issue reappears in the poll: gone_at clears, change says "back".
    let result = db.upsert_batch(&[upsert("KA-1")]).unwrap();
    assert!(result.new_keys.is_empty());
    assert_eq!(result.changed.len(), 1);
    assert!(result.changed[0].change.contains("back"));
    assert!(db.get_by_key("KA-1").unwrap().unwrap().gone_at.is_none());
}

#[test_context(InboxTestContext)]
#[serial]
#[test]
fn test_dismissed_issues_are_not_toast_candidates(_ctx: &mut InboxTestContext) {
    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[upsert("KA-1")]).unwrap();
    db.set_dismissed("KA-1", true).unwrap();

    // Dismissed issues still get gone_at but are not reported for toasts.
    let gone = db.mark_gone(&[]).unwrap();
    assert!(gone.is_empty());
    assert!(db.get_by_key("KA-1").unwrap().unwrap().gone_at.is_some());

    // Changes on dismissed issues are flagged so the caller can skip toasts.
    let mut changed = upsert("KA-1");
    changed.sort_value = Some(9.5);
    let result = db.upsert_batch(&[changed]).unwrap();
    assert_eq!(result.changed.len(), 1);
    assert!(result.changed[0].dismissed);
}

#[test]
fn test_badge_precedence_and_freshness() {
    let now = Local::now().naive_local();
    let fresh = now - Duration::hours(1);
    let stale = now - Duration::hours(48);

    let mut item = JiraInboxItem {
        issue_key: "KA-1".to_string(),
        issue_id: "1".to_string(),
        summary: "Badge probe".to_string(),
        status_id: None,
        status_name: String::new(),
        priority: None,
        priority_rank: 999,
        sort_value: None,
        url: String::new(),
        first_seen: stale,
        last_seen: fresh,
        notified: true,
        pinned: false,
        dismissed: false,
        raw_updated: None,
        gone_at: None,
        last_change: None,
        changed_at: None,
    };

    // Old, unchanged issue: no badge.
    assert_eq!(item.badge(now), None);

    // Fresh discovery: NEW.
    item.first_seen = fresh;
    assert_eq!(item.badge(now).as_deref(), Some("NEW"));

    // Fresh change on an old issue: the change text.
    item.first_seen = stale;
    item.last_change = Some("status→Done".to_string());
    item.changed_at = Some(fresh);
    assert_eq!(item.badge(now).as_deref(), Some("status→Done"));

    // Stale change: badge expires.
    item.changed_at = Some(stale);
    assert_eq!(item.badge(now), None);

    // Gone wins over everything.
    item.gone_at = Some(fresh);
    item.changed_at = Some(fresh);
    assert_eq!(item.badge(now).as_deref(), Some("gone"));
}
