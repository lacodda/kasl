//! Jira inbox command: sync, list, pin, dismiss, open, and import issues.
//!
//! Manages the local `jira_inbox` table populated from assigned open Jira issues.

use crate::db::jira_inbox::{JiraInbox, JiraInboxItem};
use crate::libs::config::Config;
use crate::libs::inbox_filter::{self, InboxFilter, InboxSort, parse_window, priority_cut_named};
use crate::libs::jira_inbox as inbox_lib;
use crate::libs::messages::Message;
use crate::libs::pick::{self, TriageAction, TriageAnswer};
use crate::libs::toast_apply::{self, Applied};
use crate::libs::view::View;
use crate::{msg_error, msg_info, msg_print, msg_success};
use anyhow::{Result, bail};
use chrono::Local;
use clap::{Args, Subcommand, ValueEnum};

/// Command-line arguments for the inbox command.
#[derive(Debug, Args)]
pub struct InboxArgs {
    #[command(subcommand)]
    command: Option<InboxCommand>,

    // Kept at the top level so the bare `kasl inbox -n 5` form keeps working
    // as a shorthand for `kasl inbox list -n 5`.
    /// Show only the top N issues after filtering and sorting
    #[arg(long, short = 'n', value_name = "N")]
    limit: Option<usize>,

    /// Include issues gone from Jira (closed or reassigned)
    #[arg(long)]
    all: bool,

    /// Include issues that are still asleep, with the date they are due back
    #[arg(long)]
    snoozed: bool,

    #[command(flatten)]
    filter: FilterArgs,
}

/// The cuts an inbox of two hundred issues needs before it is a list.
///
/// Shared by `list` and by every picker: a `take` that offers all two
/// hundred is as useless as a list of them.
#[derive(Debug, Args, Clone, Default)]
pub struct FilterArgs {
    /// Only issues first seen within a window: 1d, 7d, 12h, 2w
    #[arg(long, value_name = "WINDOW")]
    since: Option<String>,

    /// Only issues discovered in the last day (same window as the NEW badge)
    #[arg(long, conflicts_with = "since")]
    new: bool,

    /// Only issues that visibly changed within a window: 1d, 7d, 12h, 2w
    #[arg(long, value_name = "WINDOW")]
    changed: Option<String>,

    /// Only issues whose ranking field (e.g. Scoring) is at least N
    #[arg(long, value_name = "N")]
    min_score: Option<f64>,

    /// Only issues of this priority; add + for that priority and above (High+)
    #[arg(long, value_name = "NAME[+]")]
    priority: Option<String>,

    /// Only issues in this status (name as Jira shows it)
    #[arg(long, value_name = "NAME")]
    status: Option<String>,

    /// Order of the list; pinned issues lead whatever the order
    #[arg(long, value_enum, value_name = "KEY")]
    sort: Option<SortKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SortKey {
    /// Ranking field, then priority (the default)
    Score,
    /// Most urgent first
    Priority,
    /// Most recently discovered first
    New,
    /// Most recently changed first
    Changed,
}

impl From<SortKey> for InboxSort {
    fn from(key: SortKey) -> Self {
        match key {
            SortKey::Score => InboxSort::Score,
            SortKey::Priority => InboxSort::Priority,
            SortKey::New => InboxSort::New,
            SortKey::Changed => InboxSort::Changed,
        }
    }
}

impl FilterArgs {
    /// Fields set on the subcommand win; the rest fall back to the top level.
    fn or(self, fallback: FilterArgs) -> FilterArgs {
        FilterArgs {
            since: self.since.or(fallback.since),
            new: self.new || fallback.new,
            changed: self.changed.or(fallback.changed),
            min_score: self.min_score.or(fallback.min_score),
            priority: self.priority.or(fallback.priority),
            status: self.status.or(fallback.status),
            sort: self.sort.or(fallback.sort),
        }
    }

    /// Resolves the words into a filter, against the issues at hand.
    fn to_filter(&self, items: &[JiraInboxItem]) -> Result<InboxFilter> {
        let since = match (&self.since, self.new) {
            (Some(window), _) => Some(parse_window(window)?),
            (None, true) => Some(chrono::Duration::hours(crate::db::jira_inbox::FRESH_BADGE_HOURS)),
            (None, false) => None,
        };
        Ok(InboxFilter {
            since,
            changed: self.changed.as_deref().map(parse_window).transpose()?,
            min_score: self.min_score,
            priority: self.priority.as_deref().map(|name| priority_cut_named(items, name)).transpose()?,
            status: self.status.clone(),
        })
    }

    /// The cuts, in words, for the list header.
    fn describe(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if let Some(window) = &self.since {
            parts.push(format!("since {window}"));
        } else if self.new {
            parts.push("new".to_string());
        }
        if let Some(window) = &self.changed {
            parts.push(format!("changed {window}"));
        }
        if let Some(score) = self.min_score {
            parts.push(format!("score ≥ {score}"));
        }
        if let Some(priority) = &self.priority {
            parts.push(format!("priority {priority}"));
        }
        if let Some(status) = &self.status {
            parts.push(format!("status {status}"));
        }
        if let Some(sort) = self.sort {
            parts.push(format!("by {}", sort.to_possible_value().map(|v| v.get_name().to_string()).unwrap_or_default()));
        }
        parts
    }
}

/// Available inbox operations.
#[derive(Debug, Subcommand)]
enum InboxCommand {
    /// Sync assigned open issues from Jira now
    #[command(about = "Sync inbox from Jira")]
    Sync,

    /// List active (non-dismissed) inbox issues
    #[command(about = "List active inbox issues")]
    List {
        /// Show only the top N issues after filtering and sorting
        #[arg(long, short = 'n', value_name = "N")]
        limit: Option<usize>,

        /// Include issues gone from Jira (closed or reassigned)
        #[arg(long)]
        all: bool,

        /// Include issues that are still asleep, with the date they are due back
        #[arg(long)]
        snoozed: bool,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Pin an issue so it stays on top
    #[command(about = "Pin an inbox issue")]
    Pin {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Unpin a previously pinned issue
    #[command(about = "Unpin an inbox issue")]
    Unpin {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Dismiss an issue, hiding it from the list
    #[command(about = "Dismiss an inbox issue")]
    Dismiss {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Open an issue in the browser
    #[command(about = "Open issue URL in browser")]
    Open {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Walk the inbox one issue at a time, deciding each
    #[command(about = "Triage the inbox issue by issue")]
    Triage {
        /// How long `snooze` sleeps during this run: 3d, 12h, 2w
        #[arg(long, value_name = "FOR", default_value = "1d")]
        snooze_for: String,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Show one issue in full, and why it sits where it does
    #[command(about = "Show one inbox issue")]
    Show {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        /// Explain where the ranking comes from and what each mark means
        #[arg(long)]
        why: bool,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Put an issue to sleep until a moment passes
    #[command(about = "Snooze an inbox issue")]
    Snooze {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        /// How long to sleep: 3d, 12h, 2w (default 1d)
        #[arg(value_name = "FOR")]
        duration: Option<String>,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Wake a sleeping issue now
    #[command(about = "Wake a snoozed inbox issue")]
    Unsnooze {
        /// Issue key, e.g. PROJ-123; omit to pick from the sleeping ones
        #[arg(value_name = "KEY")]
        key: Option<String>,
    },

    /// Import an issue into local tasks
    #[command(about = "Import issue into tasks")]
    Take {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        #[command(flatten)]
        filter: FilterArgs,
    },
}

/// Entry point for `kasl inbox`.
pub async fn cmd(args: InboxArgs) -> Result<()> {
    let top = args.filter;
    match args.command {
        Some(InboxCommand::Sync) => {
            let outcome = inbox_lib::sync_interactive(true).await?;
            if !outcome.skipped {
                msg_success!(Message::JiraInboxSynced {
                    fetched: outcome.fetched,
                    new_count: outcome.new_keys.len(),
                    changed: outcome.changed.len(),
                    gone: outcome.gone_keys.len(),
                });
            }
            Ok(())
        }
        Some(InboxCommand::List { limit, all, snoozed, filter }) => list_inbox(limit.or(args.limit), all || args.all, snoozed || args.snoozed, &filter.or(top)),
        Some(InboxCommand::Pin { key, filter }) => set_pinned(&resolve_key(key, "Pin which issue?", false, &filter.or(top))?, true),
        Some(InboxCommand::Unpin { key, filter }) => set_pinned(&resolve_key(key, "Unpin which issue?", true, &filter.or(top))?, false),
        Some(InboxCommand::Dismiss { key, filter }) => dismiss(&resolve_key(key, "Dismiss which issue?", false, &filter.or(top))?),
        Some(InboxCommand::Open { key, filter }) => open_issue(&resolve_key(key, "Open which issue?", false, &filter.or(top))?),
        Some(InboxCommand::Triage { snooze_for, filter }) => triage(&snooze_for, &filter.or(top)),
        Some(InboxCommand::Show { key, why, filter }) => {
            let key = resolve_key(key, "Show which issue?", false, &filter.or(top))?;
            show_issue(&key, why)
        }
        Some(InboxCommand::Snooze { key, duration, filter }) => {
            let key = resolve_key(key, "Snooze which issue?", false, &filter.or(top))?;
            snooze(&key, duration.as_deref())
        }
        Some(InboxCommand::Unsnooze { key }) => unsnooze(key),
        Some(InboxCommand::Take { key, filter }) => take_issue(&resolve_key(key, "Take which issue?", false, &filter.or(top))?),
        // Bare `kasl inbox` shows the list, as it always has.
        None => list_inbox(args.limit, args.all, args.snoozed, &top),
    }
}

/// The active issues, cut and ordered as asked. Returns the slice and the
/// size of the whole, so a cut list never reads as the whole inbox.
fn sliced(include_gone: bool, include_snoozed: bool, args: &FilterArgs) -> Result<(Vec<JiraInboxItem>, usize)> {
    let items = JiraInbox::new()?.list_active_at(include_gone, include_snoozed)?;
    let total = items.len();
    let filter = args.to_filter(&items)?;
    let mut items = filter.apply(items, Local::now().naive_local());
    if let Some(sort) = args.sort {
        inbox_filter::sort(&mut items, sort.into());
    }
    Ok((items, total))
}

/// Returns the issue key to act on, opening a picker when none was given.
///
/// `pinned_only` narrows the list for `unpin`, where offering issues that are
/// not pinned would be offering work that does nothing.
fn resolve_key(key: Option<String>, prompt: &str, pinned_only: bool, filter: &FilterArgs) -> Result<String> {
    if let Some(key) = key {
        return Ok(key);
    }
    let (items, total) = sliced(false, false, filter)?;
    if items.is_empty() && total > 0 {
        bail!("no issue matches the filter ({}) - {total} in the inbox", filter.describe().join(", "));
    }
    if pinned_only {
        let pinned: Vec<_> = items.into_iter().filter(|i| i.pinned).collect();
        if pinned.is_empty() {
            bail!("no pinned issues - `kasl inbox pin KEY` pins one");
        }
        return pick::inbox_issue(&pinned, prompt);
    }
    pick::inbox_issue(&items, prompt)
}

fn list_inbox(limit: Option<usize>, include_gone: bool, include_snoozed: bool, filter: &FilterArgs) -> Result<()> {
    let (mut items, total) = sliced(include_gone, include_snoozed, filter)?;
    if total == 0 {
        // "Empty" over an inbox that is only asleep would read as nothing to
        // do, when in fact everything in it is coming back.
        let asleep = if include_snoozed { 0 } else { JiraInbox::new()?.count_snoozed()? };
        if asleep > 0 {
            msg_info!(Message::JiraInboxAllSnoozed(asleep));
        } else {
            msg_info!(Message::JiraInboxEmpty);
        }
        return Ok(());
    }
    let cuts = filter.describe();
    if items.is_empty() {
        msg_info!(Message::JiraInboxNoMatch { total, what: cuts.join(", ") });
        return Ok(());
    }
    if let Some(n) = limit {
        items.truncate(n);
    }
    // A whole inbox is announced as such; a slice says how much of the whole
    // it is, or a filtered list reads as the inbox itself.
    if items.len() == total && cuts.is_empty() {
        msg_print!(Message::JiraInboxListHeader, true);
    } else {
        msg_print!(
            Message::JiraInboxListSliced {
                shown: items.len(),
                total,
                what: cuts.join(", ")
            },
            true
        );
    }
    View::jira_inbox(&items)
}

fn set_pinned(key: &str, pinned: bool) -> Result<()> {
    let db = JiraInbox::new()?;
    if !db.set_pinned(key, pinned)? {
        msg_error!(Message::JiraInboxNotFound(key.to_string()));
        return Ok(());
    }
    if pinned {
        msg_success!(Message::JiraInboxPinned(key.to_string()));
    } else {
        msg_success!(Message::JiraInboxUnpinned(key.to_string()));
    }
    Ok(())
}

fn dismiss(key: &str) -> Result<()> {
    match toast_apply::dismiss(key)? {
        Applied::NotFound => msg_error!(Message::JiraInboxNotFound(key.to_string())),
        _ => msg_success!(Message::JiraInboxDismissed(key.to_string())),
    }
    Ok(())
}

fn open_issue(key: &str) -> Result<()> {
    let db = JiraInbox::new()?;
    let Some(item) = db.get_by_key(key)? else {
        msg_error!(Message::JiraInboxNotFound(key.to_string()));
        return Ok(());
    };

    match inbox_lib::open_url(&item.url) {
        Ok(()) => msg_success!(Message::JiraInboxOpened(key.to_string())),
        Err(e) => msg_error!(Message::JiraInboxOpenFailed(e.to_string())),
    }
    Ok(())
}

/// Walks the filtered inbox, asking what to do with each issue.
///
/// The point is the whole pile in one sitting: two hundred issues are not
/// triaged by running `take`, `snooze` and `dismiss` two hundred times, each
/// re-reading the list to find the next row. The cuts apply first, so
/// `triage --since 7d --min-score 5` walks exactly that slice.
///
/// Every action goes through the same function the standalone command calls,
/// so triage cannot drift from `inbox take` in what it actually does. The list
/// is a snapshot taken once: actions are by key, so a row going stale mid-run
/// changes nothing, and re-reading between every question would make the order
/// shift under the user's hands.
fn triage(snooze_for: &str, filter: &FilterArgs) -> Result<()> {
    // Parsed before the first question rather than at the first snooze: being
    // told the duration is unreadable after deciding twenty issues would be
    // the worst possible moment to find out.
    inbox_filter::parse_window(snooze_for)?;

    let (items, total) = sliced(false, false, filter)?;
    if total == 0 {
        msg_info!(Message::JiraInboxEmpty);
        return Ok(());
    }
    if items.is_empty() {
        msg_info!(Message::JiraInboxNoMatch {
            total,
            what: filter.describe().join(", ")
        });
        return Ok(());
    }

    let count = items.len();
    let mut done = TriageTally::default();
    for (index, item) in items.iter().enumerate() {
        // Opening is a look, not a decision, so the issue is asked about again
        // once the browser is up - otherwise "let me see it first" would
        // silently mean "skip". Asked once: a second `open` means "I have
        // looked", and reopening the same page would be a loop with no way
        // forward.
        let action = match pick::triage_action(item, index + 1, count)? {
            TriageAnswer::Decided(action) => action,
            TriageAnswer::Open => {
                open_issue(&item.issue_key)?;
                match pick::triage_action(item, index + 1, count)? {
                    TriageAnswer::Decided(action) => action,
                    TriageAnswer::Open => TriageAction::Skip,
                }
            }
        };

        match action {
            TriageAction::Take => {
                take_issue(&item.issue_key)?;
                done.taken += 1;
            }
            TriageAction::Snooze => {
                snooze(&item.issue_key, Some(snooze_for))?;
                done.snoozed += 1;
            }
            TriageAction::Dismiss => {
                dismiss(&item.issue_key)?;
                done.dismissed += 1;
            }
            TriageAction::Skip => done.skipped += 1,
            TriageAction::Quit => {
                done.left = count - index;
                break;
            }
        }
    }

    msg_success!(Message::JiraInboxTriaged {
        taken: done.taken,
        snoozed: done.snoozed,
        dismissed: done.dismissed,
        skipped: done.skipped,
        left: done.left,
    });
    Ok(())
}

/// What a triage run came to, for the closing line.
#[derive(Default)]
struct TriageTally {
    taken: usize,
    snoozed: usize,
    dismissed: usize,
    skipped: usize,
    /// Issues never asked about, because the run was stopped early.
    left: usize,
}

/// Shows one issue in full, optionally explaining its place in the list.
///
/// A row in the table is six narrow columns; this is where the whole summary,
/// the URL and the dates fit. `--why` adds where the ordering comes from,
/// which is the question a list sorted by a field nobody can see invites.
fn show_issue(key: &str, why: bool) -> Result<()> {
    let db = JiraInbox::new()?;
    let Some(item) = db.get_by_key(key)? else {
        msg_error!(Message::JiraInboxNotFound(key.to_string()));
        return Ok(());
    };

    View::jira_inbox_issue(&item)?;
    if why {
        // The label lives in config, not on the row: the number means nothing
        // without the name of the field it was read from.
        let label = Config::read()
            .ok()
            .and_then(|c| c.jira_inbox)
            .and_then(|cfg| cfg.sort_by_field.and_then(|id| score_field_label(&cfg.custom_fields, &id)));
        let reasons = inbox_filter::explain(&item, label.as_deref(), Local::now().naive_local());
        View::jira_inbox_why(&reasons)?;
    }
    Ok(())
}

/// The human name of the ranking field, when the user gave it one.
fn score_field_label(fields: &[crate::libs::config::JiraCustomField], id: &str) -> Option<String> {
    fields
        .iter()
        .find(|f| f.id.trim() == id.trim())
        .map(|f| f.label.clone())
        .or_else(|| Some(id.to_string()))
}

/// Puts an issue to sleep for a while.
///
/// The default is a day: the common case is "not today", and a snooze that
/// demanded a duration every time would be slower than reading past the row.
fn snooze(key: &str, duration: Option<&str>) -> Result<()> {
    let window = inbox_filter::parse_window(duration.unwrap_or("1d"))?;
    match toast_apply::snooze(key, window)? {
        Applied::Snoozed(until) => msg_success!(Message::JiraInboxSnoozed(key.to_string(), until.format("%b %-d %H:%M").to_string())),
        _ => msg_error!(Message::JiraInboxNotFound(key.to_string())),
    }
    Ok(())
}

/// Wakes a sleeping issue before its time.
///
/// The picker offers only sleeping issues: waking an awake one does nothing,
/// and offering it would be offering work with no effect.
fn unsnooze(key: Option<String>) -> Result<()> {
    let db = JiraInbox::new()?;
    let key = match key {
        Some(key) => key,
        None => {
            let now = Local::now().naive_local();
            let asleep: Vec<JiraInboxItem> = db
                .list_active_at(false, true)?
                .into_iter()
                .filter(|i| i.snoozed_until.is_some_and(|until| until > now))
                .collect();
            if asleep.is_empty() {
                bail!("no issues are asleep - `kasl inbox snooze KEY 3d` puts one to sleep");
            }
            pick::inbox_issue(&asleep, "Wake which issue?")?
        }
    };

    if !db.set_snoozed(&key, None)? {
        msg_error!(Message::JiraInboxNotFound(key.clone()));
        return Ok(());
    }
    msg_success!(Message::JiraInboxUnsnoozed(key));
    Ok(())
}

/// Turns an inbox issue into a task and records that it was taken.
///
/// `take` used to create the task and dismiss the issue, which severed the
/// two: the key survived only inside the task's name, and the inbox forgot the
/// issue had ever been picked up. Now the task carries `jira_key`, and the
/// issue stays in the list wearing a `taken` badge.
fn take_issue(key: &str) -> Result<()> {
    match toast_apply::take(key)? {
        Applied::Taken => msg_success!(Message::JiraInboxTaken(key.to_string())),
        Applied::AlreadyTaken(name) => msg_info!(Message::JiraInboxAlreadyTaken(key.to_string(), name)),
        _ => msg_error!(Message::JiraInboxNotFound(key.to_string())),
    }
    Ok(())
}
