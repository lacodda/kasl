//! Prints `inbox show --why` against a seeded row, so the screen is looked at
//! rather than reasoned about.
//!
//! Ignored by default: this is a probe for a human, not a gate. Run it with
//! `cargo test --test inbox_show_probe -- --ignored --nocapture`.

use chrono::{Duration, Local};
use kasl::db::jira_inbox::{JiraInbox, JiraInboxUpsert};
use kasl::libs::inbox_filter::explain;
use kasl::libs::view::View;
use serial_test::serial;
use tempfile::TempDir;

#[test]
#[ignore]
#[serial]
fn print_show_and_why() {
    let temp_dir = TempDir::new().unwrap();
    // SAFETY: single-threaded probe
    unsafe {
        std::env::set_var("HOME", temp_dir.path());
        std::env::set_var("LOCALAPPDATA", temp_dir.path());
    }

    let db = JiraInbox::new().unwrap();
    db.upsert_batch(&[JiraInboxUpsert {
        issue_key: "PROJ-4471".to_string(),
        issue_id: "1".to_string(),
        summary: "Reconcile the ledger export with the monthly close".to_string(),
        status_id: Some("10".to_string()),
        status_name: "In Progress".to_string(),
        priority: Some("High".to_string()),
        priority_rank: 2,
        sort_value: Some(8.0),
        url: "https://jira.example.com/browse/PROJ-4471".to_string(),
        raw_updated: None,
    }])
    .unwrap();
    db.set_pinned("PROJ-4471", true).unwrap();
    db.set_snoozed("PROJ-4471", Some(Local::now().naive_local() + Duration::days(2))).unwrap();

    let item = db.list_active_at(false, true).unwrap().into_iter().next().unwrap();
    View::jira_inbox_issue(&item).unwrap();
    View::jira_inbox_why(&explain(&item, Some("Scoring"), Local::now().naive_local())).unwrap();
}
