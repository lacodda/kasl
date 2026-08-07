//! Jira inbox sync and desktop toast helpers.
//!
//! Polls assigned open issues, upserts them into `jira_inbox`, and optionally
//! shows desktop toast notifications for newly discovered keys. Toast click
//! opens the issue browse URL (Windows: win-toast-notify protocol activation;
//! other platforms: notify-rust action callback).

use crate::api::jira::Jira;
use crate::db::jira_inbox::{JiraInbox, JiraInboxItem, JiraInboxUpsert, UpsertBatchResult};
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
            priority: issue.fields.priority.as_ref().map(|p| p.name.clone()),
            priority_rank: Jira::priority_rank(&issue.fields.priority),
            sort_value,
            url: jira.issue_browse_url(&issue.key),
            raw_updated: issue.fields.updated.clone(),
        });
    }

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

fn toast_body(item: &JiraInboxItem) -> String {
    let priority = item.priority.as_deref().unwrap_or("—");
    match item.sort_value {
        Some(score) => format!("[score {score}] [{priority}] {}", item.summary),
        None => format!("[{priority}] {}", item.summary),
    }
}

#[cfg(windows)]
fn show_toast_windows(item: &JiraInboxItem) -> bool {
    let title = format!("Jira {}", item.issue_key);
    let body = toast_body(item);

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
    let body = toast_body(item);
    let url = item.url.clone();
    let key = item.issue_key.clone();

    match notify_rust::Notification::new().summary(&title).body(&body).action("default", "Open").show() {
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
                    outcome.fetched,
                    outcome.new_keys.len(),
                    outcome.updated,
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
