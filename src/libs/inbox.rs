//! Inbox sync and desktop toast helpers.
//!
//! Polls every source for the issues assigned now - kasl's own Jira, and each
//! plugin that is an inbox source ([`crate::libs::inbox_source`]) - upserts
//! them into `inbox`, and optionally shows desktop toast notifications for
//! newly discovered keys. Each source is reconciled on its own: its poll
//! decides which of its rows are gone, never another source's. Toast click
//! opens the issue browse URL (Windows: win-toast-notify protocol activation;
//! other platforms: notify-rust action callback).
//!
//! A toast about an issue also carries the three decisions worth making about
//! it - Take, Snooze, Dismiss - so the pile can be triaged without opening a
//! terminal. How a button press gets back to the daemon is
//! [`crate::libs::toast_action`]; what it then does is
//! [`crate::libs::toast_apply`].
//!
//! Each poll also runs the inbox rules ([`crate::libs::inbox_rules`]) over
//! the issues it brought: a rule may pin an issue, hide it, or turn its toast
//! into an urgent one that stays on screen.

use crate::api::jira::{Jira, SessionMode};
use crate::db::inbox::{ChangedIssue, Inbox, InboxItem, InboxUpsert, Include, JIRA_SOURCE, LeftReason, UpsertBatchResult};
use crate::db::inbox_statuses::InboxStatuses;
use crate::libs::config::{Config, InboxConfig};
use crate::libs::inbox_rules::{self, RuleBook, RuleProblem, Ruled};
use crate::libs::inbox_source;
use crate::libs::messages::Message;
use crate::libs::plugin::Plugin;
use crate::libs::plugin::events;
use crate::libs::plugin::manifest::Event;
use crate::libs::toast_action::{Mailbox, ToastAction};
use crate::libs::toast_apply;
use crate::{msg_info, msg_warning};
use anyhow::Result;
use chrono::{Duration, Local, NaiveDateTime};
use std::collections::{HashMap, VecDeque};
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
/// Urgent toasts - the ones a rule asks for - count in a lane of their own,
/// with the same bound: an afternoon of ordinary changes must not have spent
/// the room an escalation needs, and an escalation of fifty issues at once is
/// still a storm.
///
/// Held in memory by the poller. A restart forgets the count, which costs at
/// most one more budget's worth, and keeps the budget from outliving the
/// process that spends it.
#[derive(Debug, Default)]
pub struct ToastBudget {
    ordinary: Lane,
    urgent: Lane,
}

impl ToastBudget {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decides how `count` ordinary toasts from one poll at `now` are shown, and spends the budget.
    pub fn admit(&mut self, now: NaiveDateTime, count: usize) -> ToastPlan {
        self.ordinary.admit(now, count)
    }

    /// The same decision for urgent toasts, from their own lane.
    pub fn admit_urgent(&mut self, now: NaiveDateTime, count: usize) -> ToastPlan {
        self.urgent.admit(now, count)
    }
}

/// One kind of toast's spending over the last hour.
#[derive(Debug, Default)]
struct Lane {
    shown: VecDeque<NaiveDateTime>,
    summary_at: Option<NaiveDateTime>,
}

impl Lane {
    fn admit(&mut self, now: NaiveDateTime, count: usize) -> ToastPlan {
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
    New(InboxItem),
    Changed(InboxItem, String),
    Gone(InboxItem),
    /// A taken issue Jira resolved while its task is still open.
    Resolved(InboxItem),
    /// An issue a rule calls urgent, with the rule's name.
    Urgent(InboxItem, String),
}

/// What a poll has to say about one issue that is still in the inbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// It arrived.
    New,
    /// A person moved it: the change in words.
    Changed(String),
    /// A rule calls it urgent: the rule's name.
    Urgent(String),
}

/// Which toast each issue of a poll gets: one at most.
///
/// `fresh` are new issues not yet announced, `changed` the notable changes
/// worth a toast, `urgent` the issues a rule has just called urgent. An urgent
/// issue gets the urgent toast in place of its new or changed one - two toasts
/// about one issue in the same breath would be one too many, and the urgent
/// one says more. Urgent is not held back by `notify_changes`: the rule is an
/// explicit ask for this toast.
pub fn notices(fresh: &[String], changed: &[(String, String)], urgent: &[(String, String)]) -> Vec<(String, Notice)> {
    let urgent_by = |key: &str| urgent.iter().find(|(k, _)| k == key).map(|(_, rule)| rule.clone());
    let mut out: Vec<(String, Notice)> = Vec::new();
    for key in fresh {
        let notice = urgent_by(key).map(Notice::Urgent).unwrap_or(Notice::New);
        out.push((key.clone(), notice));
    }
    for (key, change) in changed {
        if !fresh.contains(key) && urgent_by(key).is_none() {
            out.push((key.clone(), Notice::Changed(change.clone())));
        }
    }
    for (key, rule) in urgent {
        if !fresh.contains(key) {
            out.push((key.clone(), Notice::Urgent(rule.clone())));
        }
    }
    out
}

/// The line a summary toast opens with: what arrived, by kind.
fn summary_line(pending: &[Pending]) -> String {
    let count = |f: fn(&Pending) -> bool| pending.iter().filter(|p| f(p)).count();
    let parts = [
        (count(|p| matches!(p, Pending::New(_))), "new"),
        (count(|p| matches!(p, Pending::Changed(..))), "changed"),
        (count(|p| matches!(p, Pending::Gone(_))), "left the inbox"),
        (count(|p| matches!(p, Pending::Resolved(_))), "resolved with your task open"),
        (count(|p| matches!(p, Pending::Urgent(..))), "urgent"),
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
    /// Taken issues Jira said were resolved this pass, whose tasks are still open.
    ///
    /// The ones to offer marking done. Learnt once per departure, so a sync
    /// that reports one here will not report it again.
    pub resolved: Vec<ResolvedIssue>,
    /// What the inbox rules did this pass.
    pub ruled: Vec<Ruled>,
    /// Rules that could not be read, and were skipped.
    pub rule_problems: Vec<RuleProblem>,
    /// Keys this source listed that another source already holds, with that source.
    pub refused: Vec<(String, String)>,
    pub notified: usize,
    /// True when sync was skipped (no jira config / disabled / no credentials).
    pub skipped: bool,
}

/// A taken issue Jira resolved, with the task that is still open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIssue {
    pub issue_key: String,
    pub summary: String,
    /// The resolution Jira gave ("Done", "Won't Fix").
    pub resolution: Option<String>,
}

/// How many departed taken issues one poll asks Jira about.
///
/// The first poll after an upgrade finds every taken issue that ever left
/// unexplained; one request each would hold that poll for minutes. The rest
/// are asked about on the polls that follow.
const DEPARTURES_PER_POLL: usize = 20;

/// Asks Jira why each taken issue that left the inbox left, and keeps the answer.
///
/// Returns the ones resolved while their task is still open. An answer that
/// does not come - VPN down, Jira busy - leaves the row unexplained, and the
/// next poll asks again; a sync is not failed for it, because the issues that
/// did arrive are already reconciled and that is the part that matters most.
async fn explain_departures(jira: &mut Jira, mode: SessionMode) -> Result<Vec<ResolvedIssue>> {
    let db = Inbox::new()?;
    let mut resolved = Vec::new();
    for key in db.unexplained_departures(DEPARTURES_PER_POLL)? {
        let state = match jira.issue_state(&key, mode).await {
            Ok(state) => state,
            Err(e) => {
                warn!("Jira inbox: could not learn why {} left: {}", key, e);
                continue;
            }
        };
        match state.and_then(|s| s.resolution) {
            Some(resolution) => {
                db.set_left(&key, LeftReason::Resolved, Some(&resolution))?;
                if toast_apply::task_open(&key)?
                    && let Some(item) = db.get_by_key(&key)?
                {
                    resolved.push(ResolvedIssue {
                        issue_key: key.clone(),
                        summary: item.summary,
                        resolution: Some(resolution),
                    });
                }
            }
            None => {
                db.set_left(&key, LeftReason::Moved, None)?;
            }
        }
    }
    Ok(resolved)
}

/// What one source's sync came to, named by the source.
#[derive(Debug)]
pub struct SourceSync {
    /// [`JIRA_SOURCE`] or the plugin's name.
    pub source: String,
    pub outcome: Result<SyncOutcome>,
}

/// Runs one sync of every source, by hand (may prompt for the Jira password).
///
/// `kasl inbox sync` always fetches even when the poller is disabled in the
/// config; toasts respect `inbox.notify`. A sync run by hand gets a budget of
/// its own: it is one poll, asked for. One source failing does not stop the
/// others - each answers for itself.
pub async fn sync_interactive(notify: bool) -> Result<Vec<SourceSync>> {
    let config = Config::read()?;
    let inbox_cfg = config.inbox.clone().unwrap_or_default();
    let allow_toast = notify && inbox_cfg.notify;
    let mut budget = ToastBudget::new();
    let mut synced = Vec::new();

    if let Some(jira_config) = config.jira.clone() {
        let outcome = async {
            let mut jira = Jira::new(&jira_config);
            let issues = jira.get_assigned_open_issues(&inbox_cfg.extra_field_ids()).await?;
            // The caller is a terminal: resolved issues are asked about there, not toasted.
            let poll = Poll {
                mode: SessionMode::Interactive,
                notify: allow_toast,
                offer_by_toast: false,
            };
            let polled = jira_poll(&jira, &issues, &inbox_cfg)?;
            apply_issues(Some(&mut jira), polled, &inbox_cfg, poll, &mut budget).await
        }
        .await;
        synced.push(SourceSync {
            source: JIRA_SOURCE.to_string(),
            outcome,
        });
    }

    for plugin in inbox_source::sources()? {
        let poll = Poll {
            mode: SessionMode::Interactive,
            notify: allow_toast,
            offer_by_toast: false,
        };
        let source = plugin.name.clone();
        let outcome = sync_plugin_with(plugin, &inbox_cfg, poll, &mut budget).await;
        synced.push(SourceSync { source, outcome });
    }

    if synced.is_empty() {
        msg_warning!(Message::InboxHasNoSource);
    }
    Ok(synced)
}

/// Who is polling, and so where its news goes.
#[derive(Debug, Clone, Copy)]
struct Poll {
    mode: SessionMode,
    /// Whether toasts are shown at all.
    notify: bool,
    /// Whether a resolved issue's offer goes on a toast, or back to the caller.
    offer_by_toast: bool,
}

/// Runs Jira's unattended poll, for the background watcher.
///
/// `budget` is the poller's, carried from poll to poll, so the toasts it
/// allows are counted across polls rather than within one - and across
/// sources, since one person reads them all.
pub async fn sync_jira(inbox_cfg: &InboxConfig, budget: &mut ToastBudget) -> Result<SyncOutcome> {
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

    let poll = Poll {
        mode: SessionMode::Unattended,
        notify: inbox_cfg.notify,
        offer_by_toast: true,
    };
    let polled = jira_poll(&jira, &issues, inbox_cfg)?;
    apply_issues(Some(&mut jira), polled, inbox_cfg, poll, budget).await
}

/// Runs one plugin source's unattended poll, for the background watcher.
pub async fn sync_plugin(plugin: Plugin, inbox_cfg: &InboxConfig, budget: &mut ToastBudget) -> Result<SyncOutcome> {
    if !inbox_cfg.enabled {
        return Ok(SyncOutcome {
            skipped: true,
            ..Default::default()
        });
    }
    let poll = Poll {
        mode: SessionMode::Unattended,
        notify: inbox_cfg.notify,
        offer_by_toast: true,
    };
    sync_plugin_with(plugin, inbox_cfg, poll, budget).await
}

async fn sync_plugin_with(plugin: Plugin, inbox_cfg: &InboxConfig, poll: Poll, budget: &mut ToastBudget) -> Result<SyncOutcome> {
    let source = plugin.name.clone();
    let listing = tokio::task::spawn_blocking(move || inbox_source::list_issues(&plugin))
        .await
        .map_err(|error| anyhow::anyhow!("the plugin `{source}` stopped while listing: {error}"))??;
    let polled = plugin_poll(&source, listing)?;
    apply_issues(None, polled, inbox_cfg, poll, budget).await
}

/// One source's poll, turned into rows: what `apply_issues` reconciles.
struct Polled {
    source: String,
    upserts: Vec<InboxUpsert>,
    /// What a summary toast opens: the page that lists these issues at the source.
    list_url: String,
}

/// Jira's issues as inbox rows.
fn jira_poll(jira: &Jira, issues: &[crate::api::jira::JiraIssue], inbox_cfg: &InboxConfig) -> Result<Polled> {
    let statuses = InboxStatuses::new()?;
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

        upserts.push(InboxUpsert {
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
            issue_type: issue.fields.issuetype.as_ref().map(|t| t.name.clone()),
            // Jira's own answer, falling back to the key's prefix, which is
            // what a Jira key is.
            project: issue
                .fields
                .project
                .as_ref()
                .map(|p| p.key.clone())
                .or_else(|| issue.key.split_once('-').map(|(project, _)| project.to_string())),
            source: JIRA_SOURCE.to_string(),
        });
    }
    Ok(Polled {
        source: JIRA_SOURCE.to_string(),
        upserts,
        list_url: jira.open_issues_url(),
    })
}

/// A plugin's listing as inbox rows.
///
/// A priority the plugin names without a rank takes the rank the inbox
/// already knows for that name, so a `High+` rule reads a GitLab "High" as
/// it reads Jira's - and the vocabulary is not rewritten to "unranked" by a
/// source that simply did not say.
fn plugin_poll(source: &str, listing: inbox_source::Listing) -> Result<Polled> {
    let statuses = InboxStatuses::new()?;
    let known: HashMap<String, i32> = Inbox::new()?.priorities()?.into_iter().collect();
    let first_url = listing.issues.first().map(|issue| issue.url.clone()).unwrap_or_default();
    let mut upserts = Vec::with_capacity(listing.issues.len());
    for issue in listing.issues {
        let status_id = match issue.status.as_deref().map(str::trim).filter(|status| !status.is_empty()) {
            Some(status) => {
                let id = inbox_source::status_id(source, status);
                statuses.upsert(&id, status)?;
                Some(id)
            }
            None => None,
        };
        let priority_rank = issue
            .priority_rank
            .or_else(|| issue.priority.as_ref().and_then(|name| known.get(name).copied()))
            .unwrap_or(UNRANKED);
        upserts.push(InboxUpsert {
            issue_id: issue.id.unwrap_or_else(|| issue.key.clone()),
            issue_key: issue.key,
            summary: issue.summary,
            status_id,
            status_name: issue.status.unwrap_or_default(),
            priority: issue.priority,
            priority_rank,
            sort_value: issue.score,
            url: issue.url,
            raw_updated: issue.updated,
            issue_type: issue.issue_type,
            project: issue.project,
            source: source.to_string(),
        });
    }
    Ok(Polled {
        source: source.to_string(),
        upserts,
        list_url: listing.list_url.unwrap_or(first_url),
    })
}

/// The rank of a priority nobody ranked: after every ranked one, as Jira's
/// own fallback puts it.
const UNRANKED: i32 = 999;

/// Reconciles one source's poll with the inbox: rows, departures, rules, toasts, events.
///
/// `jira` is there for Jira's poll only: Jira is the source kasl can ask why
/// a taken issue left. A plugin's issue that leaves is simply gone.
async fn apply_issues(jira: Option<&mut Jira>, polled: Polled, inbox_cfg: &InboxConfig, poll: Poll, budget: &mut ToastBudget) -> Result<SyncOutcome> {
    let notify = poll.notify;
    let Polled { source, upserts, list_url } = polled;

    let db = Inbox::new()?;
    // A source's first poll is where kasl starts watching it, not a moment
    // two hundred issues arrived: it announces nothing to plugins. Toasts
    // keep their own rule - a storm of them collapses into one.
    let baseline = !db.has_source(&source)?;
    // Rules act on what an issue has just come to be, so what it was is
    // read before the poll overwrites it.
    let before: HashMap<String, InboxItem> = if inbox_cfg.rules.is_empty() {
        HashMap::new()
    } else {
        db.list(Include::EVERYTHING)?.into_iter().map(|item| (item.issue_key.clone(), item)).collect()
    };
    let UpsertBatchResult {
        new_keys,
        updated,
        changed,
        refused,
    } = db.upsert_batch(&upserts)?;
    for (key, owner) in &refused {
        warn!("Inbox: {} listed {}, which {} already holds; left as it was", source, key, owner);
    }

    // Reconcile: this source's issues missing from its poll are gone
    // (closed, reassigned), not frozen in the list forever. A key another
    // source holds is not this source's to keep present.
    let present_keys: Vec<String> = upserts
        .iter()
        .map(|u| u.issue_key.clone())
        .filter(|key| !refused.iter().any(|(refused, _)| refused == key))
        .collect();
    let gone_keys = db.mark_gone(&source, &present_keys)?;

    // Compiled after the upsert, so a priority name first seen in this very
    // poll is already known to a rule that names it.
    let book = RuleBook::of_inbox(inbox_cfg, &db)?;
    for problem in &book.problems {
        debug!("Inbox: rule {} is skipped: {}", problem.number, problem.reason);
    }
    let ruled = inbox_rules::apply_entering(&db, &book, &before, &present_keys)?;

    // A taken issue that left is either finished or no longer yours, and only
    // Jira knows which. The answer decides whether its task is offered as done.
    let resolved = match jira {
        Some(jira) => match explain_departures(jira, poll.mode).await {
            Ok(resolved) => resolved,
            Err(e) => {
                warn!("Jira inbox: could not explain departures: {}", e);
                Vec::new()
            }
        },
        None => Vec::new(),
    };

    // What arrived and is not hidden: a new issue a rule dismissed is not
    // news at all - hiding it was the rule's whole point.
    let arrived = db.list_unnotified_new(&new_keys)?;

    // Every toast this poll would show is gathered first and shown under
    // one decision per lane, so the budget sees the poll as a whole.
    let mut pending = Vec::new();
    if notify {
        let fresh: Vec<String> = arrived.iter().map(|i| i.issue_key.clone()).collect();
        let moved: Vec<(String, String)> = if inbox_cfg.notify_changes {
            changed
                .iter()
                .filter(|c| c.notable && !c.dismissed)
                .map(|c| (c.issue_key.clone(), c.change.clone()))
                .collect()
        } else {
            Vec::new()
        };
        let urgent: Vec<(String, String)> = ruled.iter().filter_map(|r| r.urgent.clone().map(|rule| (r.issue_key.clone(), rule))).collect();
        for (key, notice) in notices(&fresh, &moved, &urgent) {
            // Read again: a rule may have dismissed the issue in this very poll.
            if let Ok(Some(item)) = db.get_by_key(&key)
                && !item.dismissed
            {
                pending.push(match notice {
                    Notice::New => Pending::New(item),
                    Notice::Changed(change) => Pending::Changed(item, change),
                    Notice::Urgent(rule) => Pending::Urgent(item, rule),
                });
            }
        }
        // Notified means announced or deliberately not: a new issue held back
        // by the budget is in the list with its NEW badge, and toasting it an
        // hour later would be old news.
        db.mark_notified(&fresh)?;
    }
    if notify && inbox_cfg.notify_gone {
        // A resolved issue with an open task gets its own toast below; a
        // second one saying it left would be the same news twice.
        let offered: Vec<&str> = resolved.iter().map(|r| r.issue_key.as_str()).collect();
        for key in gone_keys.iter().filter(|k| !(poll.offer_by_toast && offered.contains(&k.as_str()))) {
            if let Ok(Some(item)) = db.get_by_key(key) {
                pending.push(Pending::Gone(item));
            }
        }
    }
    // Not gated by `notify_gone`: this is not news that an issue left, it is a
    // question about the person's own task.
    if notify && poll.offer_by_toast {
        for issue in &resolved {
            if let Ok(Some(item)) = db.get_by_key(&issue.issue_key) {
                pending.push(Pending::Resolved(item));
            }
        }
    }

    let (urgent_pending, ordinary): (Vec<Pending>, Vec<Pending>) = pending.into_iter().partition(|p| matches!(p, Pending::Urgent(..)));
    let now = Local::now().naive_local();
    let title = format!("{} inbox", inbox_source::label(&source));
    let mut notified = 0;
    notified += show_pending(&title, &list_url, &urgent_pending, budget.admit_urgent(now, urgent_pending.len()));
    notified += show_pending(&title, &list_url, &ordinary, budget.admit(now, ordinary.len()));

    // Read again for the same reason as the toasts: a rule may have hidden
    // an arrival in this very poll.
    if !baseline {
        for item in arrived {
            if let Ok(Some(item)) = db.get_by_key(&item.issue_key)
                && !item.dismissed
            {
                let subject = serde_json::to_value(crate::libs::json::Issue::now(&item))?;
                events::tell_async(Event::NewInboxIssue, subject).await;
            }
        }
    }

    Ok(SyncOutcome {
        fetched: upserts.len(),
        new_keys,
        updated,
        changed,
        gone_keys,
        resolved,
        ruled,
        rule_problems: book.problems,
        refused,
        notified,
        skipped: false,
    })
}

/// Shows one lane's toasts as its budget decided; returns how many appeared.
fn show_pending(title: &str, list_url: &str, pending: &[Pending], plan: ToastPlan) -> usize {
    match plan {
        ToastPlan::Each => pending
            .iter()
            .filter(|toast| match toast {
                Pending::New(item) => show_toast(item),
                Pending::Changed(item, change) => show_change_toast(item, change),
                Pending::Gone(item) => show_gone_toast(item),
                Pending::Resolved(item) => show_resolved_toast(item),
                Pending::Urgent(item, rule) => show_urgent_toast(item, rule),
            })
            .count(),
        ToastPlan::Summary => usize::from(show_summary_toast(title, &summary_line(pending), list_url)),
        ToastPlan::Quiet => {
            debug!("{}: held back {} toast(s); the hour's budget is spent", title, pending.len());
            0
        }
    }
}

/// Returns every issue whose snooze has run out, toasting each one.
///
/// Returns how many came back. A snooze the user set is a promise to be
/// reminded, so the return is announced even though the issue was already
/// known - that reminder is the whole reason to snooze rather than dismiss.
pub fn wake_snoozed(notify: bool) -> Result<usize> {
    let woken = Inbox::new()?.wake_due()?;
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
                "kasl inbox",
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
pub fn show_snoozed_toast(item: &InboxItem) -> bool {
    let body = format!("Back from snooze - {}", item.summary);
    show_issue_toast(item, &body)
}

/// Shows a desktop toast for a newly discovered inbox item.
///
/// Clicking the toast body opens [`InboxItem::url`] in the default
/// browser; the buttons decide the issue without leaving the desktop.
pub fn show_toast(item: &InboxItem) -> bool {
    show_issue_toast(item, &toast_body(item))
}

/// Shows a toast that stays: an issue a rule calls urgent.
///
/// The same three buttons as any issue toast, and the rule named in the body,
/// so the person can tell an escalation from a slow afternoon - and knows
/// which rule to change if it keeps being wrong. On Windows the toast stays
/// on screen until acted on; on Linux it is sent as critical.
pub fn show_urgent_toast(item: &InboxItem, rule: &str) -> bool {
    let body = format!("Urgent by the rule '{rule}' - {}", toast_body(item));
    show_toast_as(
        &format!("{} - urgent", toast_title(item)),
        &body,
        &item.url,
        &item.issue_key,
        &ToastAction::ALL,
        Urgency::Urgent,
    )
}

/// Shows a toast for a visible change on an existing inbox item.
pub fn show_change_toast(item: &InboxItem, change: &str) -> bool {
    let body = format!("{change} — {}", item.summary);
    show_issue_toast(item, &body)
}

/// Shows one toast standing in for many; clicking opens the source's list of issues.
///
/// No buttons: a summary is about a pile, and there is no one issue for Take
/// to take. The pile is triaged in `kasl inbox triage`, which the body says.
pub fn show_summary_toast(title: &str, what: &str, list_url: &str) -> bool {
    show_raw_toast(title, &format!("{what} - see `kasl inbox`"), list_url, "inbox", &[])
}

/// What a toast about one issue is titled: its source and its key, `Jira KA-1`.
fn toast_title(item: &InboxItem) -> String {
    format!("{} {}", inbox_source::label(&item.source), item.issue_key)
}

/// Shows a toast for an issue that left the inbox (closed or reassigned).
///
/// No buttons either: the issue is already gone, so all three decisions are
/// about work that is no longer there.
pub fn show_gone_toast(item: &InboxItem) -> bool {
    let body = format!("Left the inbox — {}", item.summary);
    show_raw_toast(&toast_title(item), &body, &item.url, &item.issue_key, &[])
}

/// Shows a toast for a taken issue Jira resolved while its task is open.
///
/// One button, **Mark done**: the issue's own decisions are behind it - it is
/// resolved - and the only question left is the person's task.
pub fn show_resolved_toast(item: &InboxItem) -> bool {
    let resolution = item.resolution.as_deref().map(|r| format!(" ({r})")).unwrap_or_default();
    let body = format!(
        "Resolved in {}{resolution} - your task is still open. {}",
        inbox_source::label(&item.source),
        item.summary
    );
    show_raw_toast(&toast_title(item), &body, &item.url, &item.issue_key, &[ToastAction::Done])
}

/// A toast about one live issue: the body opens it, the buttons decide it.
fn show_issue_toast(item: &InboxItem, body: &str) -> bool {
    show_raw_toast(&toast_title(item), body, &item.url, &item.issue_key, &ToastAction::ALL)
}

/// How insistent a toast is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Urgency {
    /// Shows, then slides into the notification centre.
    Normal,
    /// Stays until acted on, where the platform allows it.
    Urgent,
}

/// Platform dispatch for a toast with a click-to-open URL and buttons.
fn show_raw_toast(title: &str, body: &str, url: &str, key: &str, actions: &[ToastAction]) -> bool {
    show_toast_as(title, body, url, key, actions, Urgency::Normal)
}

fn show_toast_as(title: &str, body: &str, url: &str, key: &str, actions: &[ToastAction], urgency: Urgency) -> bool {
    #[cfg(windows)]
    {
        show_toast_windows(title, body, url, key, actions, urgency)
    }
    #[cfg(not(windows))]
    {
        show_toast_other(title, body, url, key, actions, urgency)
    }
}

fn toast_body(item: &InboxItem) -> String {
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
fn show_toast_windows(title: &str, body: &str, url: &str, key: &str, actions: &[ToastAction], urgency: Urgency) -> bool {
    let mut toast = win_toast_notify::WinToastNotify::new().set_title(title).set_messages(vec![body]).set_open(url);
    if let Some(logo) = toast_logo_path() {
        toast = toast.set_logo(&logo.to_string_lossy(), win_toast_notify::CropCircle::False);
    }
    // The reminder scenario keeps the toast on screen until it is acted on.
    // Not the "urgent" scenario: that one is Windows 11 only and needs a
    // per-app permission an unpackaged program does not get.
    if urgency == Urgency::Urgent {
        toast = toast.set_scenario(win_toast_notify::Scenario::Reminder);
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
fn show_toast_other(title: &str, body: &str, url: &str, key: &str, actions: &[ToastAction], urgency: Urgency) -> bool {
    let url = url.to_string();
    let owned_key = key.to_string();

    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body).action("default", "Open");
    // Critical notifications stay until dismissed on XDG desktops.
    if urgency == Urgency::Urgent {
        notification.urgency(notify_rust::Urgency::Critical);
    }
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
fn show_toast_other(title: &str, body: &str, _url: &str, key: &str, _actions: &[ToastAction], _urgency: Urgency) -> bool {
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
pub async fn drain_toast_actions() -> Result<usize> {
    let requests = Mailbox::open()?.collect()?;
    if requests.is_empty() {
        return Ok(0);
    }

    let db = Inbox::new()?;
    let mut done = 0;
    for request in &requests {
        // Read the row before acting: the answering toast should still open
        // the issue it is about, and a dismiss or a take may be the last
        // moment its URL is easy to reach.
        let url = db.get_by_key(&request.issue_key).ok().flatten().map(|item| item.url).unwrap_or_default();
        match toast_apply::apply(request).await {
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
                show_raw_toast("kasl inbox", &outcome.summary(&request.issue_key), &url, &request.issue_key, &[]);
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
        match drain_toast_actions().await {
            Ok(0) => {}
            Ok(count) => debug!("Carried out {} toast action(s)", count),
            Err(e) => warn!("Jira inbox: failed to drain toast actions: {}", e),
        }
        tokio::time::sleep(std::time::Duration::from_secs(MAILBOX_INTERVAL_SECS)).await;
    }
}

/// Background poll loop used by `kasl watch`.
///
/// Polls Jira when the `jira` block and the `inbox` section are both there
/// (the inbox section is how Jira's poll was always switched on), and every
/// plugin source that is set up - installing and setting one up is the
/// switch. `inbox.enabled: false` stops all of them. Re-reads the config each
/// wake, so `kasl setup` and `kasl plugin setup` take effect without a
/// restart of the watcher.
pub async fn run_poller() {
    let mut budget = ToastBudget::new();
    loop {
        let config = match Config::read() {
            Ok(c) => c,
            Err(e) => {
                warn!("Inbox: failed to read config: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }
        };

        let inbox_cfg = config.inbox.clone().unwrap_or_default();
        let jira_polls = config.jira.is_some() && config.inbox.is_some();
        let plugins = if inbox_cfg.enabled {
            match inbox_source::sources() {
                Ok(plugins) => plugins,
                Err(e) => {
                    warn!("Inbox: could not look for plugin sources: {}", e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        if !inbox_cfg.enabled || (!jira_polls && plugins.is_empty()) {
            tokio::time::sleep(std::time::Duration::from_secs(inbox_cfg.poll_interval_secs.max(60))).await;
            continue;
        }

        // Waking is local bookkeeping and runs before the sync, on its own:
        // an issue deferred to Monday is due on Monday whether or not its
        // source answers, and hanging the return off a successful poll would
        // mean a VPN outage silently held issues asleep past their moment.
        match wake_snoozed(inbox_cfg.notify) {
            Ok(count) if count > 0 => msg_info!(Message::InboxWoke(count)),
            Ok(_) => {}
            Err(e) => warn!("Inbox wake error: {}", e),
        }

        if jira_polls {
            report_poll(JIRA_SOURCE, sync_jira(&inbox_cfg, &mut budget).await);
        }
        for plugin in plugins {
            let source = plugin.name.clone();
            report_poll(&source, sync_plugin(plugin, &inbox_cfg, &mut budget).await);
        }

        let secs = inbox_cfg.poll_interval_secs.max(30);
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
    }
}

/// Writes one source's poll to the watcher's log.
fn report_poll(source: &str, outcome: Result<SyncOutcome>) {
    match outcome {
        Ok(outcome) if !outcome.skipped => {
            if !outcome.new_keys.is_empty() {
                msg_info!(Message::InboxNewIssues(outcome.new_keys.len()));
            }
            debug!(
                "Inbox sync of {}: fetched={}, new={}, updated={}, changed={}, gone={}, refused={}, notified={}",
                source,
                outcome.fetched,
                outcome.new_keys.len(),
                outcome.updated,
                outcome.changed.len(),
                outcome.gone_keys.len(),
                outcome.refused.len(),
                outcome.notified
            );
        }
        Ok(_) => {}
        Err(e) => warn!("Inbox sync of {} failed: {:#}", source, e),
    }
}
