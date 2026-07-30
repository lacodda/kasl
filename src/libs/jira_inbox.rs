//! Jira inbox sync and desktop toast helpers.
//!
//! Polls assigned open issues, upserts them into `jira_inbox`, and optionally
//! shows desktop toast notifications for newly discovered keys. Toast click
//! opens the issue browse URL (Windows: win-toast-notify protocol activation;
//! other platforms: notify-rust action callback).

use crate::api::jira::Jira;
use crate::db::jira_inbox::{JiraInbox, JiraInboxItem, JiraInboxUpsert, UpsertBatchResult};
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

    let allow_toast = notify
        && config
            .jira_inbox
            .as_ref()
            .map(|c| c.notify)
            .unwrap_or(true);

    let mut jira = Jira::new(&jira_config);
    let issues = jira.get_assigned_open_issues().await?;
    apply_issues(&jira, &issues, allow_toast).await
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
    let Some(issues) = jira.get_assigned_open_issues_noninteractive().await? else {
        warn!("Jira inbox poll skipped: no cached session or secret");
        return Ok(SyncOutcome {
            skipped: true,
            ..Default::default()
        });
    };

    apply_issues(&jira, &issues, inbox_cfg.notify).await
}

async fn apply_issues(jira: &Jira, issues: &[crate::api::jira::JiraIssue], notify: bool) -> Result<SyncOutcome> {
    let upserts: Vec<JiraInboxUpsert> = issues
        .iter()
        .map(|issue| JiraInboxUpsert {
            issue_key: issue.key.clone(),
            issue_id: issue.id.clone(),
            summary: issue.fields.summary.clone(),
            status: issue.fields.status.name.clone(),
            priority: issue.fields.priority.as_ref().map(|p| p.name.clone()),
            priority_rank: Jira::priority_rank(&issue.fields.priority),
            url: jira.issue_browse_url(&issue.key),
            raw_updated: issue.fields.updated.clone(),
        })
        .collect();

    let db = JiraInbox::new()?;
    let UpsertBatchResult { new_keys, updated } = db.upsert_batch(&upserts)?;

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

    Ok(SyncOutcome {
        fetched: issues.len(),
        new_keys,
        updated,
        notified,
        skipped: false,
    })
}

/// Shows a desktop toast for a newly discovered inbox item.
///
/// Clicking the toast opens [`JiraInboxItem::url`] in the default browser.
pub fn show_toast(item: &JiraInboxItem) -> bool {
    #[cfg(windows)]
    {
        show_toast_windows(item)
    }
    #[cfg(not(windows))]
    {
        show_toast_other(item)
    }
}

#[cfg(windows)]
fn show_toast_windows(item: &JiraInboxItem) -> bool {
    let title = format!("Jira {}", item.issue_key);
    let priority = item.priority.as_deref().unwrap_or("—");
    let body = format!("[{}] {}", priority, item.summary);

    match win_toast_notify::WinToastNotify::new()
        .set_title(&title)
        .set_messages(vec![&body])
        .set_open(&item.url)
        .show()
    {
        Ok(()) => {
            debug!("Showed toast for {}", item.issue_key);
            true
        }
        Err(e) => {
            warn!("Failed to show toast for {}: {}", item.issue_key, e);
            false
        }
    }
}

#[cfg(not(windows))]
fn show_toast_other(item: &JiraInboxItem) -> bool {
    let title = format!("Jira {}", item.issue_key);
    let priority = item.priority.as_deref().unwrap_or("—");
    let body = format!("[{}] {}", priority, item.summary);
    let url = item.url.clone();
    let key = item.issue_key.clone();

    match notify_rust::Notification::new()
        .summary(&title)
        .body(&body)
        .action("default", "Open")
        .show()
    {
        Ok(handle) => {
            // Wait for click off the poller thread so sync stays responsive.
            std::thread::spawn(move || {
                handle.wait_for_action(|action| {
                    if action == "default" {
                        if let Err(e) = open_url(&url) {
                            warn!("Failed to open {} from toast: {}", key, e);
                        }
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
/// `kasl init` can enable polling without restarting the watcher in most cases
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
                    "Jira inbox sync: fetched={}, new={}, updated={}, notified={}",
                    outcome.fetched, outcome.new_keys.len(), outcome.updated, outcome.notified
                );
            }
            Ok(_) => {}
            Err(e) => warn!("Jira inbox sync error: {}", e),
        }

        let secs = inbox_cfg.poll_interval_secs.max(30);
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    }
}
