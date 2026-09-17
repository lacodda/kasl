//! Manual probe: pops a real toast with real buttons, against a real inbox row.
//!
//! Green tests prove the mailbox and the shortcut writer. They cannot prove
//! that Windows renders the buttons, that pressing one launches the shortcut,
//! or that the shell accepts the URI - all three live outside the process. So
//! this probe puts a toast on screen and prints what to watch for.
//!
//! Run: `cargo test --test toast_button_probe -- --ignored --nocapture`
#![cfg(windows)]

use kasl::db::jira_inbox::{JiraInbox, JiraInboxUpsert};
use kasl::libs::toast_action::{Mailbox, ToastAction};

/// Shows a toast for a seeded issue and waits for a press to land.
///
/// The row is seeded through the real `upsert_batch`, and the toast through
/// the real `show_toast`, so what appears on screen is what a Jira poll would
/// raise - a rewritten copy of either would be the thing that silently
/// drifts.
#[test]
#[ignore = "pops a real desktop toast and waits for a click; run manually"]
fn a_toast_button_reaches_the_mailbox() {
    let db = JiraInbox::new().unwrap();
    let key = "KA-9001";
    db.upsert_batch(&[JiraInboxUpsert {
        issue_key: key.to_string(),
        issue_id: "9001".to_string(),
        summary: "Toast button probe - press a button on this toast".to_string(),
        status_id: Some("10".to_string()),
        status_name: "Open".to_string(),
        priority: Some("High".to_string()),
        priority_rank: 2,
        sort_value: Some(7.0),
        url: "https://kasl.lacodda.com".to_string(),
        raw_updated: None,
    }])
    .unwrap();

    let item = db.get_by_key(key).unwrap().expect("the probe row must exist");

    // Start from an empty mailbox, or a leftover press would read as this one.
    let mailbox = Mailbox::open().unwrap();
    let _ = mailbox.collect();

    println!("\n--- toast button probe ---");
    println!("Mailbox: {}", mailbox.path().display());
    println!("Watch for: a toast titled 'Jira {key}' with three buttons - Take, Snooze, Dismiss.");
    println!("Then press one. Nothing should flash on screen when you do.");
    println!("Shortcuts written:");
    for action in ToastAction::ALL {
        match kasl::libs::toast_shortcut::ensure(action, key) {
            Ok(uri) => println!("  {:>8}  {uri}", action.label()),
            Err(e) => println!("  {:>8}  FAILED: {e}", action.label()),
        }
    }

    assert!(kasl::libs::jira_inbox::show_toast(&item), "the toast itself must appear");
    println!("\nToast shown. Waiting up to 60s for a press...");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut seen = Vec::new();
    while std::time::Instant::now() < deadline {
        seen = mailbox.collect().unwrap();
        if !seen.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    assert!(
        !seen.is_empty(),
        "no press reached the mailbox in 60s. Either no button was pressed, or the \
         shortcut did not launch the courier - check the shortcut directory by hand."
    );

    for request in &seen {
        println!("Received: {} {}", request.action.as_str(), request.issue_key);
        assert_eq!(request.issue_key, key, "the press must name the issue the toast was about");
    }

    // And the whole way through: the daemon side carries it out.
    let done = kasl::libs::jira_inbox::drain_toast_actions().unwrap();
    println!("Drain carried out {done} action(s); a second toast should now say what happened.");
}
