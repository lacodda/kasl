//! Jira inbox sync and desktop toast helpers.
//!
//! Polls assigned open issues, upserts them into `jira_inbox`, and optionally
//! shows desktop toast notifications for newly discovered keys. Toast click
//! opens the issue browse URL (Windows: win-toast-notify protocol activation;
//! other platforms: notify-rust action callback).

use crate::api::jira::Jira;
use crate::db::jira_inbox::{ChangedIssue, JiraInbox, JiraInboxItem, JiraInboxUpsert, UpsertBatchResult};
use crate::db::jira_statuses::JiraStatuses;
use crate::libs::config::{Config, JiraInboxConfig};
use crate::libs::messages::Message;
use crate::{msg_info, msg_warning};
use anyhow::Result;
use std::process::Command;
use tracing::{debug, warn};

/// Outcome of a single inbox sync pass.
#[derive(Debug, Default)]
pub struct SyncOutcome {
    pub fetched: usize,
    pub new_keys: Vec<String>,
    pub updated: usize,
    /// Existing issues whose status/priority/score visibly changed.
    pub changed: Vec<ChangedIssue>,
    /// Issues that stopped appearing in the poll this pass.
    pub gone_keys: Vec<String>,
    pub notified: usize,
    /// True when sync was skipped (no jira config / disabled / no credentials).
    pub skipped: bool,
}

/// Runs one interactive sync (may prompt for Jira password).
///
/// `--sync` always fetches even when the inbox poller is disabled in config;
/// toasts respect `jira_inbox.notify` when that section exists.
pub async fn sync_interactive(notify: bool) -> Result<SyncOutcome> {
    let config = Config::read()?;
    let Some(jira_config) = config.jira.clone() else {
        msg_warning!(Message::JiraInboxRequiresJiraConfig);
        return Ok(SyncOutcome {
            skipped: true,
            ..Default::default()
        });
    };

    let inbox_cfg = config.jira_inbox.clone().unwrap_or_default();
    let allow_toast = notify && inbox_cfg.notify;

    let mut jira = Jira::new(&jira_config);
    let issues = jira.get_assigned_open_issues(&inbox_cfg.extra_field_ids()).await?;
    apply_issues(&jira, &issues, &inbox_cfg, allow_toast).await
}

/// Runs one non-interactive sync for the background watcher.
pub async fn sync_noninteractive(inbox_cfg: &JiraInboxConfig) -> Result<SyncOutcome> {
    if !inbox_cfg.enabled {
        return Ok(SyncOutcome {
            skipped: true,
            ..Default::default()
        });
    }

    let config = Config::read()?;
    let Some(jira_config) = config.jira.clone() else {
        return Ok(SyncOutcome {
            skipped: true,
            ..Default::default()
        });
    };

    let mut jira = Jira::new(&jira_config);
    let Some(issues) = jira.get_assigned_open_issues_noninteractive(&inbox_cfg.extra_field_ids()).await? else {
        warn!("Jira inbox poll skipped: no cached session or secret");
        return Ok(SyncOutcome {
            skipped: true,
            ..Default::default()
        });
    };

    apply_issues(&jira, &issues, inbox_cfg, inbox_cfg.notify).await
}

async fn apply_issues(jira: &Jira, issues: &[crate::api::jira::JiraIssue], inbox_cfg: &JiraInboxConfig, notify: bool) -> Result<SyncOutcome> {
    let statuses = JiraStatuses::new()?;
    let sort_field = inbox_cfg.sort_by_field.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let mut upserts = Vec::with_capacity(issues.len());
    for issue in issues {
        if !issue.fields.status.id.is_empty() {
            statuses.upsert(&issue.fields.status.id, &issue.fields.status.name)?;
        }

        let status_id = if issue.fields.status.id.is_empty() {
            None
        } else {
            Some(issue.fields.status.id.clone())
        };

        let sort_value = sort_field.and_then(|id| Jira::sort_value_from_issue(issue, id));

        upserts.push(JiraInboxUpsert {
            issue_key: issue.key.clone(),
            issue_id: issue.id.clone(),
            summary: issue.fields.summary.clone(),
            status_id,
            status_name: issue.fields.status.name.clone(),
            priority: issue.fields.priority.as_ref().map(|p| p.name.clone()),
            priority_rank: Jira::priority_rank(&issue.fields.priority),
            sort_value,
            url: jira.issue_browse_url(&issue.key),
            raw_updated: issue.fields.updated.clone(),
        });
    }

    let db = JiraInbox::new()?;
    let UpsertBatchResult { new_keys, updated, changed } = db.upsert_batch(&upserts)?;

    // Reconcile: issues missing from this poll are gone (closed, reassigned),
    // not frozen in the list forever.
    let present_keys: Vec<String> = upserts.iter().map(|u| u.issue_key.clone()).collect();
    let gone_keys = db.mark_gone(&present_keys)?;

    let mut notified = 0;
    if notify && !new_keys.is_empty() {
        let to_notify = db.list_unnotified_new(&new_keys)?;
        for item in &to_notify {
            if show_toast(item) {
                notified += 1;
            }
        }
        let keys: Vec<String> = to_notify.iter().map(|i| i.issue_key.clone()).collect();
        db.mark_notified(&keys)?;
    }

    if notify && inbox_cfg.notify_changes {
        for change in changed.iter().filter(|c| !c.dismissed) {
            if let Ok(Some(item)) = db.get_by_key(&change.issue_key)
                && show_change_toast(&item, &change.change)
            {
                notified += 1;
            }
        }
    }

    if notify && inbox_cfg.notify_gone {
        for key in &gone_keys {
            if let Ok(Some(item)) = db.get_by_key(key)
                && show_gone_toast(&item)
            {
                notified += 1;
            }
        }
    }

    Ok(SyncOutcome {
        fetched: issues.len(),
        new_keys,
        updated,
        changed,
        gone_keys,
        notified,
        skipped: false,
    })
}

/// Shows a desktop toast for a newly discovered inbox item.
///
/// Clicking the toast opens [`JiraInboxItem::url`] in the default browser.
pub fn show_toast(item: &JiraInboxItem) -> bool {
    show_raw_toast(&format!("Jira {}", item.issue_key), &toast_body(item), &item.url, &item.issue_key)
}

/// Shows a toast for a visible change on an existing inbox item.
pub fn show_change_toast(item: &JiraInboxItem, change: &str) -> bool {
    let body = format!("{change} — {}", item.summary);
    show_raw_toast(&format!("Jira {}", item.issue_key), &body, &item.url, &item.issue_key)
}

/// Shows a toast for an issue that left the inbox (closed or reassigned).
pub fn show_gone_toast(item: &JiraInboxItem) -> bool {
    let body = format!("Left the inbox — {}", item.summary);
    show_raw_toast(&format!("Jira {}", item.issue_key), &body, &item.url, &item.issue_key)
}

/// Platform dispatch for a toast with a click-to-open URL.
fn show_raw_toast(title: &str, body: &str, url: &str, key: &str) -> bool {
    #[cfg(windows)]
    {
        show_toast_windows(title, body, url, key)
    }
    #[cfg(not(windows))]
    {
        show_toast_other(title, body, url, key)
    }
}

fn toast_body(item: &JiraInboxItem) -> String {
    let priority = item.priority.as_deref().unwrap_or("—");
    match item.sort_value {
        Some(score) => format!("[score {score}] [{priority}] {}", item.summary),
        None => format!("[{priority}] {}", item.summary),
    }
}

/// Materializes the embedded brand logo for toast notifications.
///
/// Toast XML references images by file path, so the PNG compiled into the
/// binary is written to the data directory on first use. A logo failure only
/// degrades the toast, so all errors collapse to `None`.
#[cfg(windows)]
fn toast_logo_path() -> Option<std::path::PathBuf> {
    const LOGO: &[u8] = include_bytes!("../../assets/toast-96.png");
    let path = crate::libs::data_storage::DataStorage::new().get_path("toast-logo.png").ok()?;
    // Rewrite when the embedded logo changes (e.g. after a self-update).
    if std::fs::metadata(&path).map(|m| m.len() != LOGO.len() as u64).unwrap_or(true) {
        std::fs::write(&path, LOGO).ok()?;
    }
    Some(path)
}

#[cfg(windows)]
fn show_toast_windows(title: &str, body: &str, url: &str, key: &str) -> bool {
    let mut toast = win_toast_notify::WinToastNotify::new().set_title(title).set_messages(vec![body]).set_open(url);
    if let Some(logo) = toast_logo_path() {
        toast = toast.set_logo(&logo.to_string_lossy(), win_toast_notify::CropCircle::False);
    }

    match toast.show() {
        Ok(()) => {
            debug!("Showed toast for {}", key);
            true
        }
        Err(e) => {
            warn!("Failed to show toast for {}: {}", key, e);
            false
        }
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn show_toast_other(title: &str, body: &str, url: &str, key: &str) -> bool {
    let url = url.to_string();
    let owned_key = key.to_string();

    match notify_rust::Notification::new().summary(title).body(body).action("default", "Open").show() {
        Ok(handle) => {
            // Wait for click off the poller thread so sync stays responsive.
            std::thread::spawn(move || {
                handle.wait_for_action(|action| {
                    if action == "default"
                        && let Err(e) = open_url(&url)
                    {
                        warn!("Failed to open {} from toast: {}", owned_key, e);
                    }
                });
            });
            debug!("Showed toast for {}", key);
            true
        }
        Err(e) => {
            warn!("Failed to show toast for {}: {}", key, e);
            false
        }
    }
}

/// macOS: notify-rust cannot wait for notification clicks (no actions API),
/// so the toast is display-only and opening stays on the CLI (`inbox --open`).
#[cfg(target_os = "macos")]
fn show_toast_other(title: &str, body: &str, _url: &str, key: &str) -> bool {
    match notify_rust::Notification::new().summary(title).body(body).show() {
        Ok(_) => {
            debug!("Showed toast for {}", key);
            true
        }
        Err(e) => {
            warn!("Failed to show toast for {}: {}", key, e);
            false
        }
    }
}

/// Opens a URL in the platform default browser / handler.
pub fn open_url(url: &str) -> Result<()> {
    #[cfg(windows)]
    {
        Command::new("cmd").args(["/C", "start", "", url]).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}

/// Background poll loop used by `kasl watch`.
///
/// Idle when `jira_inbox` is absent or disabled; re-reads config each wake so
/// `kasl setup` can enable polling without restarting the watcher in most cases
/// (restart still recommended after config changes).
pub async fn run_poller() {
    loop {
        let config = match Config::read() {
            Ok(c) => c,
            Err(e) => {
                warn!("Jira inbox: failed to read config: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }
        };

        let Some(inbox_cfg) = config.jira_inbox.clone() else {
            // Section not configured — do not poll until user runs init.
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            continue;
        };

        if !inbox_cfg.enabled || config.jira.is_none() {
            tokio::time::sleep(std::time::Duration::from_secs(inbox_cfg.poll_interval_secs.max(60))).await;
            continue;
        }

        match sync_noninteractive(&inbox_cfg).await {
            Ok(outcome) if !outcome.skipped => {
                if !outcome.new_keys.is_empty() {
                    msg_info!(Message::JiraInboxNewIssues(outcome.new_keys.len()));
                }
                debug!(
                    "Jira inbox sync: fetched={}, new={}, updated={}, changed={}, gone={}, notified={}",
                    outcome.fetched,
                    outcome.new_keys.len(),
                    outcome.updated,
                    outcome.changed.len(),
                    outcome.gone_keys.len(),
                    outcome.notified
                );
            }
            Ok(_) => {}
            Err(e) => warn!("Jira inbox sync error: {}", e),
        }

        let secs = inbox_cfg.poll_interval_secs.max(30);
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    }
}
