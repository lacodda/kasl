//! Jira inbox sync and desktop toast helpers.
//!
//! Polls assigned open issues, upserts them into `jira_inbox`, and optionally
//! shows desktop toast notifications for newly discovered keys. Toast click
//! opens the issue browse URL (Windows: win-toast-notify protocol activation;
//! other platforms: notify-rust action callback).
//!
//! A toast about an issue also carries the three decisions worth making about
//! it - Take, Snooze, Dismiss - so the pile can be triaged without opening a
//! terminal. How a button press gets back to the daemon is
//! [`crate::libs::toast_action`]; what it then does is
//! [`crate::libs::toast_apply`].

use crate::api::jira::Jira;
use crate::db::jira_inbox::{ChangedIssue, JiraInbox, JiraInboxItem, JiraInboxUpsert, UpsertBatchResult};
use crate::db::jira_statuses::JiraStatuses;
use crate::libs::config::{Config, JiraInboxConfig};
use crate::libs::messages::Message;
use crate::libs::toast_action::{Mailbox, ToastAction};
use crate::libs::toast_apply;
use crate::{msg_info, msg_warning};
use anyhow::Result;
use chrono::{Duration, Local, NaiveDateTime};
use std::collections::VecDeque;
use std::process::Command;
use tracing::{debug, warn};

/// Above this many toasts, they collapse into one summary toast.
///
/// The poller counts against it over an hour ([`ToastBudget`]); waking
/// snoozed issues, which the user scheduled, counts per wake.
///
/// A first sync of two hundred open issues, or a Jira-side re-scoring of all
/// of them, is one event, not two hundred; two hundred toasts teach the user
/// to switch notifications off, after which the one toast that matters is
/// never seen.
pub const TOAST_STORM_THRESHOLD: usize = 5;

/// Whether `count` toasts of one kind should become a single summary toast.
pub fn toasts_collapse(count: usize) -> bool {
    count > TOAST_STORM_THRESHOLD
}

/// The span over which [`ToastBudget`] counts the toasts it let through.
pub const TOAST_WINDOW_MINUTES: i64 = 60;

/// What one poll's toasts turn into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastPlan {
    /// Each issue gets its own toast.
    Each,
    /// One toast stands in for all of them.
    Summary,
    /// Nothing is shown; the list in `kasl inbox` carries the news.
    Quiet,
}

/// How many issue toasts the poller may show, counted over time rather than per poll.
///
/// A limit per poll does not hold against a trickle. Jira-side automation
/// that rescores two hundred issues a few at a time, or a list that keeps
/// losing and regaining issues, arrives as three or four changes on every
/// poll - under any per-poll threshold, and all of them toasted, poll after
/// poll, for as long as it lasts. Counting over the last hour makes the
/// total what is bounded: at most [`TOAST_STORM_THRESHOLD`] single toasts,
/// then one summary, then quiet until the hour has room again.
///
/// Held in memory by the poller. A restart forgets the count, which costs at
/// most one more budget's worth, and keeps the budget from outliving the
/// process that spends it.
#[derive(Debug, Default)]
pub struct ToastBudget {
    shown: VecDeque<NaiveDateTime>,
    summary_at: Option<NaiveDateTime>,
}

impl ToastBudget {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decides how `count` toasts from one poll at `now` are shown, and spends the budget.
    pub fn admit(&mut self, now: NaiveDateTime, count: usize) -> ToastPlan {
        let since = now - Duration::minutes(TOAST_WINDOW_MINUTES);
        while self.shown.front().is_some_and(|t| *t <= since) {
            self.shown.pop_front();
        }
        if self.summary_at.is_some_and(|t| t <= since) {
            self.summary_at = None;
        }

        if count == 0 {
            return ToastPlan::Each;
        }
        if self.shown.len() + count <= TOAST_STORM_THRESHOLD {
            self.shown.extend(std::iter::repeat_n(now, count));
            return ToastPlan::Each;
        }
        if self.summary_at.is_none() {
            self.summary_at = Some(now);
            return ToastPlan::Summary;
        }
        ToastPlan::Quiet
    }
}

/// A toast one poll would like to show.
enum Pending {
    New(JiraInboxItem),
    Changed(JiraInboxItem, String),
    Gone(JiraInboxItem),
}

/// The line a summary toast opens with: what arrived, by kind.
fn summary_line(pending: &[Pending]) -> String {
    let count = |f: fn(&Pending) -> bool| pending.iter().filter(|p| f(p)).count();
    let parts = [
        (count(|p| matches!(p, Pending::New(_))), "new"),
        (count(|p| matches!(p, Pending::Changed(..))), "changed"),
        (count(|p| matches!(p, Pending::Gone(_))), "left the inbox"),
    ];
    let said: Vec<String> = parts.iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{n} {what}")).collect();
    format!("Issues: {}", said.join(", "))
}

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
/// toasts respect `jira_inbox.notify` when that section exists. A sync run
/// by hand gets a budget of its own: it is one poll, asked for.
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
    apply_issues(&jira, &issues, &inbox_cfg, allow_toast, &mut ToastBudget::new()).await
}

/// Runs one non-interactive sync for the background watcher.
///
/// `budget` is the poller's, carried from poll to poll, so the toasts it
/// allows are counted across polls rather than within one.
pub async fn sync_noninteractive(inbox_cfg: &JiraInboxConfig, budget: &mut ToastBudget) -> Result<SyncOutcome> {
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

    apply_issues(&jira, &issues, inbox_cfg, inbox_cfg.notify, budget).await
}

async fn apply_issues(
    jira: &Jira,
    issues: &[crate::api::jira::JiraIssue],
    inbox_cfg: &JiraInboxConfig,
    notify: bool,
    budget: &mut ToastBudget,
) -> Result<SyncOutcome> {
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

    // Every toast this poll would show is gathered first and shown under
    // one decision, so the budget sees the poll as a whole.
    let mut pending = Vec::new();
    if notify && !new_keys.is_empty() {
        let to_notify = db.list_unnotified_new(&new_keys)?;
        let keys: Vec<String> = to_notify.iter().map(|i| i.issue_key.clone()).collect();
        pending.extend(to_notify.into_iter().map(Pending::New));
        // Notified means announced or deliberately not: a new issue held back
        // by the budget is in the list with its NEW badge, and toasting it an
        // hour later would be old news.
        db.mark_notified(&keys)?;
    }
    if notify && inbox_cfg.notify_changes {
        for change in changed.iter().filter(|c| c.notable && !c.dismissed) {
            if let Ok(Some(item)) = db.get_by_key(&change.issue_key) {
                pending.push(Pending::Changed(item, change.change.clone()));
            }
        }
    }
    if notify && inbox_cfg.notify_gone {
        for key in &gone_keys {
            if let Ok(Some(item)) = db.get_by_key(key) {
                pending.push(Pending::Gone(item));
            }
        }
    }

    let mut notified = 0;
    match budget.admit(Local::now().naive_local(), pending.len()) {
        ToastPlan::Each => {
            for toast in &pending {
                let shown = match toast {
                    Pending::New(item) => show_toast(item),
                    Pending::Changed(item, change) => show_change_toast(item, change),
                    Pending::Gone(item) => show_gone_toast(item),
                };
                if shown {
                    notified += 1;
                }
            }
        }
        ToastPlan::Summary => {
            if show_summary_toast(jira, &summary_line(&pending)) {
                notified += 1;
            }
        }
        ToastPlan::Quiet => debug!("Jira inbox: held back {} toast(s); the hour's budget is spent", pending.len()),
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

/// Returns every issue whose snooze has run out, toasting each one.
///
/// Returns how many came back. A snooze the user set is a promise to be
/// reminded, so the return is announced even though the issue was already
/// known - that reminder is the whole reason to snooze rather than dismiss.
pub fn wake_snoozed(notify: bool) -> Result<usize> {
    let woken = JiraInbox::new()?.wake_due()?;
    if woken.is_empty() {
        return Ok(0);
    }
    if notify {
        if toasts_collapse(woken.len()) {
            // A summary toast whose click did nothing would be the worst of
            // both: the base comes off an issue's own browse URL, so the list
            // opens without this reaching for the Jira config.
            let list_url = open_issues_url_from(&woken[0].url);
            show_raw_toast(
                "Jira inbox",
                &format!("{} snoozed issues are back - see `kasl inbox`", woken.len()),
                &list_url,
                "inbox",
                &[],
            );
        } else {
            for item in &woken {
                show_snoozed_toast(item);
            }
        }
    }
    Ok(woken.len())
}

/// The assigned-open-issues list, derived from an issue's browse URL.
///
/// Waking is local, so it must not need the Jira config to be readable; the
/// base is already carried on every row as `{base}/browse/{key}`. A URL that
/// does not have that shape yields the row's own URL, which still opens
/// something true.
fn open_issues_url_from(issue_url: &str) -> String {
    match issue_url.rsplit_once("/browse/") {
        Some((base, _)) => format!("{base}/issues/?jql=assignee%20%3D%20currentUser()%20AND%20resolution%20is%20EMPTY"),
        None => issue_url.to_string(),
    }
}

/// Shows a toast for an issue whose snooze has run out.
pub fn show_snoozed_toast(item: &JiraInboxItem) -> bool {
    let body = format!("Back from snooze - {}", item.summary);
    show_issue_toast(item, &body)
}

/// Shows a desktop toast for a newly discovered inbox item.
///
/// Clicking the toast body opens [`JiraInboxItem::url`] in the default
/// browser; the buttons decide the issue without leaving the desktop.
pub fn show_toast(item: &JiraInboxItem) -> bool {
    show_issue_toast(item, &toast_body(item))
}

/// Shows a toast for a visible change on an existing inbox item.
pub fn show_change_toast(item: &JiraInboxItem, change: &str) -> bool {
    let body = format!("{change} — {}", item.summary);
    show_issue_toast(item, &body)
}

/// Shows one toast standing in for many; clicking opens the open-issues list in Jira.
///
/// No buttons: a summary is about a pile, and there is no one issue for Take
/// to take. The pile is triaged in `kasl inbox triage`, which the body says.
pub fn show_summary_toast(jira: &Jira, what: &str) -> bool {
    show_raw_toast("Jira inbox", &format!("{what} - see `kasl inbox`"), &jira.open_issues_url(), "inbox", &[])
}

/// Shows a toast for an issue that left the inbox (closed or reassigned).
///
/// No buttons either: the issue is already gone, so all three decisions are
/// about work that is no longer there.
pub fn show_gone_toast(item: &JiraInboxItem) -> bool {
    let body = format!("Left the inbox — {}", item.summary);
    show_raw_toast(&format!("Jira {}", item.issue_key), &body, &item.url, &item.issue_key, &[])
}

/// A toast about one live issue: the body opens it, the buttons decide it.
fn show_issue_toast(item: &JiraInboxItem, body: &str) -> bool {
    show_raw_toast(&format!("Jira {}", item.issue_key), body, &item.url, &item.issue_key, &ToastAction::ALL)
}

/// Platform dispatch for a toast with a click-to-open URL and buttons.
fn show_raw_toast(title: &str, body: &str, url: &str, key: &str, actions: &[ToastAction]) -> bool {
    #[cfg(windows)]
    {
        show_toast_windows(title, body, url, key, actions)
    }
    #[cfg(not(windows))]
    {
        show_toast_other(title, body, url, key, actions)
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

/// The buttons for one issue, each pointing at its own shortcut.
///
/// A shortcut that cannot be written costs that button and nothing else: a
/// toast with two buttons is still useful, and a toast that failed to appear
/// because of a file permission would not be.
#[cfg(windows)]
fn windows_buttons(key: &str, actions: &[ToastAction]) -> Vec<win_toast_notify::Action> {
    actions
        .iter()
        .filter_map(|action| match crate::libs::toast_shortcut::ensure(*action, key) {
            Ok(uri) => Some(win_toast_notify::Action {
                activation_type: win_toast_notify::ActivationType::Protocol,
                action_content: action.label().to_string(),
                arguments: uri,
                image_url: None,
            }),
            Err(e) => {
                warn!("No {} button on the {} toast: {}", action.as_str(), key, e);
                None
            }
        })
        .collect()
}

#[cfg(windows)]
fn show_toast_windows(title: &str, body: &str, url: &str, key: &str, actions: &[ToastAction]) -> bool {
    let mut toast = win_toast_notify::WinToastNotify::new().set_title(title).set_messages(vec![body]).set_open(url);
    if let Some(logo) = toast_logo_path() {
        toast = toast.set_logo(&logo.to_string_lossy(), win_toast_notify::CropCircle::False);
    }
    if !actions.is_empty() {
        let buttons = windows_buttons(key, actions);
        if !buttons.is_empty() {
            toast = toast.set_actions(buttons);
        }
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

/// Linux and other XDG desktops: buttons are D-Bus actions on the
/// notification, so the press arrives in this process directly and there is
/// no shortcut and no courier - the mailbox is posted to in-process.
///
/// The wait runs on its own thread because `wait_for_action` blocks until the
/// notification is closed, and the poller must not stop for it.
#[cfg(all(not(windows), not(target_os = "macos")))]
fn show_toast_other(title: &str, body: &str, url: &str, key: &str, actions: &[ToastAction]) -> bool {
    let url = url.to_string();
    let owned_key = key.to_string();

    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body).action("default", "Open");
    for action in actions {
        notification.action(action.as_str(), action.label());
    }

    match notification.show() {
        Ok(handle) => {
            // Wait for click off the poller thread so sync stays responsive.
            std::thread::spawn(move || {
                handle.wait_for_action(|action| {
                    if action == "default" {
                        if let Err(e) = open_url(&url) {
                            warn!("Failed to open {} from toast: {}", owned_key, e);
                        }
                    } else if let Some(chosen) = ToastAction::parse(action) {
                        let request = crate::libs::toast_action::ToastRequest::new(chosen, &owned_key);
                        // Posted rather than applied here so both platforms
                        // settle a decision in exactly one place: the drain.
                        match Mailbox::open().and_then(|m| m.post(&request)) {
                            Ok(()) => debug!("Posted {} {} from a toast button", action, owned_key),
                            Err(e) => warn!("Failed to post {} {} from a toast button: {}", action, owned_key, e),
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

/// macOS: notify-rust cannot wait for notification clicks (no actions API),
/// so the toast is display-only, and both opening and deciding stay on the
/// CLI (`kasl inbox triage`). The buttons are not rendered rather than
/// rendered dead: a button that does nothing is worse than no button.
#[cfg(target_os = "macos")]
fn show_toast_other(title: &str, body: &str, _url: &str, key: &str, _actions: &[ToastAction]) -> bool {
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

/// Performs every toast button press waiting in the mailbox.
///
/// Returns how many were carried out. Each one answers on a toast, because
/// the decision was made on a toast: a button that changes the database in
/// silence leaves the user unsure whether the press registered, and pressing
/// again is the natural response to that doubt.
///
/// One failing request does not stop the rest. A click is a user action that
/// already happened; refusing to carry out the next one because the previous
/// key was missing would lose work the user did.
pub fn drain_toast_actions() -> Result<usize> {
    let requests = Mailbox::open()?.collect()?;
    if requests.is_empty() {
        return Ok(0);
    }

    let db = JiraInbox::new()?;
    let mut done = 0;
    for request in &requests {
        // Read the row before acting: the answering toast should still open
        // the issue it is about, and a dismiss or a take may be the last
        // moment its URL is easy to reach.
        let url = db.get_by_key(&request.issue_key).ok().flatten().map(|item| item.url).unwrap_or_default();
        match toast_apply::apply(request) {
            Ok(outcome) => {
                debug!("Toast action {} {}: {:?}", request.action.as_str(), request.issue_key, outcome);
                if outcome.settled() {
                    // The buttons of a decided issue are shortcuts that still
                    // run; nothing points at them any more, so they go.
                    #[cfg(windows)]
                    crate::libs::toast_shortcut::forget(&request.issue_key);
                }
                msg_info!(Message::ToastActionApplied(outcome.summary(&request.issue_key)));
                // The answer opens the issue, not nothing: an empty launch
                // target makes the toast body a dead click area, which reads
                // as the toast being broken.
                show_raw_toast("Jira inbox", &outcome.summary(&request.issue_key), &url, &request.issue_key, &[]);
                done += 1;
            }
            Err(e) => {
                warn!("Toast action {} {} failed: {}", request.action.as_str(), request.issue_key, e);
                msg_warning!(Message::ToastActionFailed(request.issue_key.clone(), e.to_string()));
            }
        }
    }
    Ok(done)
}

/// How often the daemon looks in the toast mailbox.
///
/// A button has to answer in the time a person waits after pressing one, so
/// this is seconds, not the poll interval. It cannot ride on the Jira poll:
/// that runs every five minutes by default, and a button that answers in
/// five minutes is not a button - the user would press it again, or conclude
/// it does not work.
const MAILBOX_INTERVAL_SECS: u64 = 2;

/// Watches the toast mailbox and performs what lands in it.
///
/// Its own loop rather than a step of the Jira poll, and it keeps running
/// when polling is disabled or Jira is unreachable: a press on a toast that
/// is already on screen is a decision about a local row, and owes nothing to
/// the network.
pub async fn run_mailbox_watcher() {
    loop {
        match drain_toast_actions() {
            Ok(0) => {}
            Ok(count) => debug!("Carried out {} toast action(s)", count),
            Err(e) => warn!("Jira inbox: failed to drain toast actions: {}", e),
        }
        tokio::time::sleep(std::time::Duration::from_secs(MAILBOX_INTERVAL_SECS)).await;
    }
}

/// Background poll loop used by `kasl watch`.
///
/// Idle when `jira_inbox` is absent or disabled; re-reads config each wake so
/// `kasl setup` can enable polling without restarting the watcher in most cases
/// (restart still recommended after config changes).
pub async fn run_poller() {
    let mut budget = ToastBudget::new();
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

        // Waking is local bookkeeping and runs before the sync, on its own:
        // an issue deferred to Monday is due on Monday whether or not Jira
        // answers, and hanging the return off a successful poll would mean a
        // VPN outage silently held issues asleep past their moment.
        match wake_snoozed(inbox_cfg.notify) {
            Ok(count) if count > 0 => msg_info!(Message::JiraInboxWoke(count)),
            Ok(_) => {}
            Err(e) => warn!("Jira inbox wake error: {}", e),
        }

        match sync_noninteractive(&inbox_cfg, &mut budget).await {
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
