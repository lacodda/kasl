//! Manual probe: pops a real desktop toast to eyeball the brand logo.
//! Run: cargo test --test toast_probe -- --ignored
#![cfg(windows)]

use kasl::db::inbox::InboxItem;

#[test]
#[ignore = "pops a real desktop toast; run manually"]
fn toast_shows_with_logo() {
    let now = chrono::Local::now().naive_local();
    let item = InboxItem {
        issue_key: "KA-1".into(),
        issue_id: "1".into(),
        summary: "Toast logo probe".into(),
        status_id: None,
        status_name: String::new(),
        priority: Some("High".into()),
        priority_rank: 0,
        sort_value: None,
        url: "https://kasl.lacodda.com".into(),
        first_seen: now,
        last_seen: now,
        notified: false,
        pinned: false,
        dismissed: false,
        raw_updated: None,
        gone_at: None,
        last_change: None,
        changed_at: None,
        taken_at: None,
        snoozed_until: None,
        woke_at: None,
        left_reason: None,
        resolution: None,
        issue_type: None,
        project: None,
        ruled_by: None,
    };
    assert!(kasl::libs::inbox::show_toast(&item));
}
