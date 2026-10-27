//! Jira inbox command: sync, list, pin, dismiss, open, and import issues -
//! the two moves kasl makes in Jira itself, start and close - and the rules
//! that pin, hide and tag issues without being asked each time.
//!
//! Manages the local `inbox` table populated from assigned open Jira issues.

use crate::db::inbox::{Inbox, InboxItem, Include};
use crate::libs::config::Config;
use crate::libs::inbox::{self as inbox_lib, ResolvedIssue, SourceSync};
use crate::libs::inbox_filter::{self, InboxFilter, InboxSort, parse_window, priority_cut_in};
use crate::libs::inbox_rules::{self, RuleBook};
use crate::libs::inbox_source;
use crate::libs::jira_bridge::{self, Goal, MoveOptions};
use crate::libs::json;
use crate::libs::messages::Message;
use crate::libs::output::{self, Format};
use crate::libs::pick::{self, TriageAction, TriageAnswer};
use crate::libs::prompt::is_interactive;
use crate::libs::toast_apply::{self, Applied};
use crate::libs::view::View;
use crate::{msg_error, msg_error_anyhow, msg_info, msg_print, msg_success, msg_warning};
use anyhow::{Result, bail};
use chrono::Local;
use clap::{Args, Subcommand, ValueEnum};
use dialoguer::{Input, MultiSelect, theme::ColorfulTheme};

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

    /// Include dismissed issues, and who dismissed them
    #[arg(long)]
    dismissed: bool,

    #[command(flatten)]
    filter: FilterArgs,

    #[command(flatten)]
    format: Format,
}

/// The cuts an inbox of two hundred issues needs before it is a list.
///
/// Shared by `list` and by every picker: a `take` that offers all two
/// hundred is as useless as a list of them.
///
/// The MCP `inbox` tool builds one from its arguments, which carry the same
/// names, so a cut means the same thing typed or asked for.
#[derive(Debug, Args, Clone, Default)]
pub struct FilterArgs {
    /// Only issues first seen within a window: 1d, 7d, 12h, 2w
    #[arg(long, value_name = "WINDOW")]
    pub(crate) since: Option<String>,

    /// Only issues discovered in the last day (same window as the NEW badge)
    #[arg(long, conflicts_with = "since")]
    pub(crate) new: bool,

    /// Only issues that visibly changed within a window: 1d, 7d, 12h, 2w
    #[arg(long, value_name = "WINDOW")]
    pub(crate) changed: Option<String>,

    /// Only issues whose ranking field (e.g. Scoring) is at least N
    #[arg(long, value_name = "N")]
    pub(crate) min_score: Option<f64>,

    /// Only issues of this priority; add + for that priority and above (High+)
    #[arg(long, value_name = "NAME[+]")]
    pub(crate) priority: Option<String>,

    /// Only issues in this status (name as Jira shows it); repeat for any of several
    #[arg(long, value_name = "NAME")]
    pub(crate) status: Vec<String>,

    /// Only issues of this type (Bug, Story - as Jira names them); repeat for any of several
    #[arg(long = "type", value_name = "NAME")]
    pub(crate) issue_type: Vec<String>,

    /// Only issues of this project (the prefix of the key); repeat for any of several
    #[arg(long, value_name = "KEY")]
    pub(crate) project: Vec<String>,

    /// Order of the list; pinned issues lead whatever the order
    #[arg(long, value_enum, value_name = "KEY")]
    pub(crate) sort: Option<SortKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum SortKey {
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
            status: either(self.status, fallback.status),
            issue_type: either(self.issue_type, fallback.issue_type),
            project: either(self.project, fallback.project),
            sort: self.sort.or(fallback.sort),
        }
    }

    /// Resolves the words into a filter, against the priorities the inbox knows.
    fn to_filter(&self, priorities: &[(String, i32)]) -> Result<InboxFilter> {
        let since = match (&self.since, self.new) {
            (Some(window), _) => Some(parse_window(window)?),
            (None, true) => Some(chrono::Duration::hours(crate::db::inbox::FRESH_BADGE_HOURS)),
            (None, false) => None,
        };
        Ok(InboxFilter {
            since,
            changed: self.changed.as_deref().map(parse_window).transpose()?,
            min_score: self.min_score,
            priority: self.priority.as_deref().map(|name| priority_cut_in(priorities, name)).transpose()?,
            status: self.status.clone(),
            types: self.issue_type.clone(),
            projects: self.project.clone(),
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
        for (what, names) in [("status", &self.status), ("type", &self.issue_type), ("project", &self.project)] {
            if !names.is_empty() {
                parts.push(format!("{what} {}", names.join(" or ")));
            }
        }
        if let Some(sort) = self.sort {
            parts.push(format!("by {}", sort.to_possible_value().map(|v| v.get_name().to_string()).unwrap_or_default()));
        }
        parts
    }
}

/// A repeated flag on the subcommand wins whole; otherwise the top level's.
fn either(own: Vec<String>, fallback: Vec<String>) -> Vec<String> {
    if own.is_empty() { fallback } else { own }
}

/// Available inbox operations.
#[derive(Debug, Subcommand)]
enum InboxCommand {
    /// Sync assigned open issues from Jira now
    #[command(about = "Sync the inbox from every source")]
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

        /// Include dismissed issues, and who dismissed them
        #[arg(long)]
        dismissed: bool,

        #[command(flatten)]
        filter: FilterArgs,

        #[command(flatten)]
        format: Format,
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

    /// Bring a dismissed issue back into the list
    #[command(about = "Undismiss an inbox issue")]
    Undismiss {
        /// Issue key, e.g. PROJ-123; omit to pick from the dismissed ones
        #[arg(value_name = "KEY")]
        key: Option<String>,
    },

    /// Show the inbox rules and what each matches now
    #[command(about = "Show and apply inbox rules")]
    Rules {
        /// Apply the rules to the issues already in the inbox
        #[arg(long)]
        apply: bool,

        #[command(flatten)]
        format: Format,
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

        #[command(flatten)]
        format: Format,
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

    /// Move an issue into progress in Jira
    #[command(about = "Start an issue at its source")]
    Start {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        /// Target status or transition name, when Jira offers several ways
        #[arg(long, value_name = "NAME")]
        to: Option<String>,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Resolve an issue in Jira, optionally leaving a comment first
    #[command(about = "Close an issue at its source")]
    Close {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        /// Comment to add before closing
        #[arg(long, short = 'm', value_name = "TEXT")]
        comment: Option<String>,

        /// Target status or transition name, when Jira offers several ways
        #[arg(long, value_name = "NAME")]
        to: Option<String>,

        /// Resolution, when the close asks for one (Done, Fixed, Won't Fix)
        #[arg(long, value_name = "NAME")]
        resolution: Option<String>,

        #[command(flatten)]
        filter: FilterArgs,
    },

    /// Add a comment to an issue in Jira
    #[command(about = "Comment on an issue at its source")]
    Comment {
        /// Issue key, e.g. PROJ-123; omit to pick from the inbox
        #[arg(value_name = "KEY")]
        key: Option<String>,

        /// The comment; omit to type it on a terminal
        #[arg(value_name = "TEXT")]
        text: Option<String>,

        #[command(flatten)]
        filter: FilterArgs,
    },
}

/// Entry point for `kasl inbox`.
pub async fn cmd(args: InboxArgs) -> Result<()> {
    let top = args.filter;
    // The top-level flags are the bare form's, `kasl inbox --json`. Before a
    // subcommand that does not list, `--json` would be dropped on the floor,
    // and a script asking for JSON must not be answered in prose.
    let top_json = args.format.json;
    if top_json && !matches!(args.command, None | Some(InboxCommand::List { .. })) {
        bail!("--json before a subcommand applies to the list; give it to the subcommand: `kasl inbox show KEY --json`");
    }
    match args.command {
        Some(InboxCommand::Sync) => {
            let synced = inbox_lib::sync_interactive(true).await?;
            let mut failed = 0;
            for SourceSync { source, outcome } in synced {
                let label = inbox_source::label(&source);
                match outcome {
                    Ok(outcome) if !outcome.skipped => {
                        msg_success!(Message::InboxSynced {
                            source: label.clone(),
                            fetched: outcome.fetched,
                            new_count: outcome.new_keys.len(),
                            changed: outcome.changed.len(),
                            gone: outcome.gone_keys.len(),
                        });
                        if !outcome.refused.is_empty() {
                            let keys: Vec<String> = outcome.refused.iter().map(|(key, owner)| format!("{key} ({owner})")).collect();
                            msg_warning!(Message::InboxRefusedKeys {
                                source: label,
                                keys: keys.join(", "),
                            });
                        }
                        report_rules(&outcome.ruled, &outcome.rule_problems);
                        offer_resolved(&outcome.resolved)?;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        failed += 1;
                        msg_error!(Message::InboxSyncFailed {
                            source: label,
                            error: format!("{error:#}"),
                        });
                    }
                }
            }
            if failed > 0 {
                bail!("{failed} inbox source(s) did not sync");
            }
            Ok(())
        }
        Some(InboxCommand::List {
            limit,
            all,
            snoozed,
            dismissed,
            filter,
            format,
        }) => {
            let include = Include {
                gone: all || args.all,
                snoozed: snoozed || args.snoozed,
                dismissed: dismissed || args.dismissed,
            };
            list_inbox(limit.or(args.limit), include, &filter.or(top), format.json || top_json)
        }
        Some(InboxCommand::Pin { key, filter }) => set_pinned(&resolve_key(key, "Pin which issue?", false, &filter.or(top))?, true),
        Some(InboxCommand::Unpin { key, filter }) => set_pinned(&resolve_key(key, "Unpin which issue?", true, &filter.or(top))?, false),
        Some(InboxCommand::Dismiss { key, filter }) => dismiss(&resolve_key(key, "Dismiss which issue?", false, &filter.or(top))?),
        Some(InboxCommand::Undismiss { key }) => undismiss(key),
        Some(InboxCommand::Rules { apply, format }) => rules(apply, format.json),
        Some(InboxCommand::Open { key, filter }) => open_issue(&resolve_key(key, "Open which issue?", false, &filter.or(top))?),
        Some(InboxCommand::Triage { snooze_for, filter }) => triage(&snooze_for, &filter.or(top)).await,
        Some(InboxCommand::Show { key, why, filter, format }) => {
            let key = resolve_key(key, "Show which issue?", false, &filter.or(top))?;
            show_issue(&key, why, format.json)
        }
        Some(InboxCommand::Snooze { key, duration, filter }) => {
            let key = resolve_key(key, "Snooze which issue?", false, &filter.or(top))?;
            snooze(&key, duration.as_deref())
        }
        Some(InboxCommand::Unsnooze { key }) => unsnooze(key),
        Some(InboxCommand::Take { key, filter }) => take_issue(&resolve_key(key, "Take which issue?", false, &filter.or(top))?).await,
        Some(InboxCommand::Start { key, to, filter }) => {
            let key = resolve_key(key, "Start which issue?", false, &filter.or(top))?;
            start_issue(&key, to).await
        }
        Some(InboxCommand::Close {
            key,
            comment,
            to,
            resolution,
            filter,
        }) => {
            let key = resolve_key(key, "Close which issue?", false, &filter.or(top))?;
            close_issue(&key, comment, to, resolution).await
        }
        Some(InboxCommand::Comment { key, text, filter }) => {
            let key = resolve_key(key, "Comment on which issue?", false, &filter.or(top))?;
            comment_issue(&key, text).await
        }
        // Bare `kasl inbox` shows the list, as it always has.
        None => {
            let include = Include {
                gone: args.all,
                snoozed: args.snoozed,
                dismissed: args.dismissed,
            };
            list_inbox(args.limit, include, &top, top_json)
        }
    }
}

/// The active issues, cut and ordered as asked. Returns the slice and the
/// size of the whole, so a cut list never reads as the whole inbox.
fn sliced(include: Include, args: &FilterArgs) -> Result<(Vec<InboxItem>, usize)> {
    let db = Inbox::new()?;
    let items = db.list(include)?;
    let total = items.len();
    let filter = args.to_filter(&db.priorities()?)?;
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
    let (items, total) = sliced(Include::default(), filter)?;
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

/// The inbox, cut and ordered as asked: what `kasl inbox list --json` prints
/// and the MCP `inbox` tool returns.
///
/// Read from the local table only, never from Jira, so it answers at once.
pub(crate) fn list_document(limit: Option<usize>, include: Include, filter: &FilterArgs) -> Result<json::InboxList> {
    let (mut items, total) = sliced(include, filter)?;
    if let Some(n) = limit {
        items.truncate(n);
    }
    let now = Local::now().naive_local();
    Ok(json::InboxList {
        issues: items.iter().map(|item| json::Issue::at(item, now)).collect(),
        total,
        filter: filter.describe(),
    })
}

/// Prints the inbox, cut and ordered as asked.
///
/// With `--json` the same slice, in the same order, as one document - read
/// from the local table only, never from Jira, so it answers at once
/// (rigger reads it this way, `kasl inbox list --all --snoozed --json`).
fn list_inbox(limit: Option<usize>, include: Include, filter: &FilterArgs, json: bool) -> Result<()> {
    warn_skipped_rules();
    if json {
        return output::print_json(&list_document(limit, include, filter)?);
    }
    let (mut items, total) = sliced(include, filter)?;
    if total == 0 {
        // "Empty" over an inbox that is only asleep would read as nothing to
        // do, when in fact everything in it is coming back.
        let asleep = if include.snoozed { 0 } else { Inbox::new()?.count_snoozed()? };
        if asleep > 0 {
            msg_info!(Message::InboxAllSnoozed(asleep));
        } else {
            msg_info!(Message::InboxEmpty);
        }
        return Ok(());
    }
    let cuts = filter.describe();
    if items.is_empty() {
        msg_info!(Message::InboxNoMatch { total, what: cuts.join(", ") });
        return Ok(());
    }
    if let Some(n) = limit {
        items.truncate(n);
    }
    // A whole inbox is announced as such; a slice says how much of the whole
    // it is, or a filtered list reads as the inbox itself.
    if items.len() == total && cuts.is_empty() {
        msg_print!(Message::InboxListHeader, true);
    } else {
        msg_print!(
            Message::InboxListSliced {
                shown: items.len(),
                total,
                what: cuts.join(", ")
            },
            true
        );
    }
    View::inbox(&items)
}

fn set_pinned(key: &str, pinned: bool) -> Result<()> {
    let db = Inbox::new()?;
    if !db.set_pinned(key, pinned)? {
        msg_error!(Message::InboxNotFound(key.to_string()));
        return Ok(());
    }
    if pinned {
        msg_success!(Message::InboxPinned(key.to_string()));
    } else {
        msg_success!(Message::InboxUnpinned(key.to_string()));
    }
    Ok(())
}

fn dismiss(key: &str) -> Result<()> {
    match toast_apply::dismiss(key)? {
        Applied::NotFound => msg_error!(Message::InboxNotFound(key.to_string())),
        _ => msg_success!(Message::InboxDismissed(key.to_string())),
    }
    Ok(())
}

fn open_issue(key: &str) -> Result<()> {
    let db = Inbox::new()?;
    let Some(item) = db.get_by_key(key)? else {
        msg_error!(Message::InboxNotFound(key.to_string()));
        return Ok(());
    };

    match inbox_lib::open_url(&item.url) {
        Ok(()) => msg_success!(Message::InboxOpened(key.to_string())),
        Err(e) => msg_error!(Message::InboxOpenFailed(e.to_string())),
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
async fn triage(snooze_for: &str, filter: &FilterArgs) -> Result<()> {
    // Parsed before the first question rather than at the first snooze: being
    // told the duration is unreadable after deciding twenty issues would be
    // the worst possible moment to find out.
    inbox_filter::parse_window(snooze_for)?;

    let (items, total) = sliced(Include::default(), filter)?;
    if total == 0 {
        msg_info!(Message::InboxEmpty);
        return Ok(());
    }
    if items.is_empty() {
        msg_info!(Message::InboxNoMatch {
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
                take_issue(&item.issue_key).await?;
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

    msg_success!(Message::InboxTriaged {
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
///
/// An issue the inbox does not hold is a failure (exit 1), so a script can
/// tell it from one it got.
fn show_issue(key: &str, why: bool, json: bool) -> Result<()> {
    if json {
        return output::print_json(&issue_document(key, why)?);
    }
    let db = Inbox::new()?;
    let Some(item) = db.get_by_key(key)? else {
        return Err(msg_error_anyhow!(Message::InboxNotFound(key.to_string())));
    };

    let reasons = if why { Some(explain_issue(&db, &item)?) } else { None };
    View::inbox_issue(&item)?;
    if let Some(reasons) = reasons {
        View::inbox_why(&reasons)?;
    }
    Ok(())
}

/// One issue as `kasl inbox show --json` prints it and the MCP `issue` tool
/// returns it; with `why`, where its place in the list comes from.
///
/// An issue the inbox does not hold is an error, not an empty answer.
pub(crate) fn issue_document(key: &str, why: bool) -> Result<json::IssueShown> {
    let db = Inbox::new()?;
    let Some(item) = db.get_by_key(key)? else {
        return Err(msg_error_anyhow!(Message::InboxNotFound(key.to_string())));
    };
    let reasons = if why { Some(explain_issue(&db, &item)?) } else { None };
    Ok(json::IssueShown {
        issue: json::Issue::now(&item),
        why: reasons.map(|reasons| reasons.iter().map(json::Reason::from).collect()),
    })
}

/// Where an issue's place in the list comes from, and what the rules make of it.
fn explain_issue(db: &Inbox, item: &InboxItem) -> Result<Vec<inbox_filter::Reason>> {
    // The label lives in config, not on the row: the number means nothing
    // without the name of the field it was read from.
    let label = Config::read()
        .ok()
        .and_then(|c| c.inbox)
        .and_then(|cfg| cfg.sort_by_field.and_then(|id| score_field_label(&cfg.custom_fields, &id)));
    let mut reasons = inbox_filter::explain(item, label.as_deref(), Local::now().naive_local());
    reasons.extend(inbox_rules::explain(&RuleBook::from_config(db)?, item));
    Ok(reasons)
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
        Applied::Snoozed(until) => msg_success!(Message::InboxSnoozed(key.to_string(), until.format("%b %-d %H:%M").to_string())),
        _ => msg_error!(Message::InboxNotFound(key.to_string())),
    }
    Ok(())
}

/// Wakes a sleeping issue before its time.
///
/// The picker offers only sleeping issues: waking an awake one does nothing,
/// and offering it would be offering work with no effect.
fn unsnooze(key: Option<String>) -> Result<()> {
    let db = Inbox::new()?;
    let key = match key {
        Some(key) => key,
        None => {
            let now = Local::now().naive_local();
            let asleep: Vec<InboxItem> = db
                .list(Include {
                    snoozed: true,
                    ..Include::default()
                })?
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
        msg_error!(Message::InboxNotFound(key.clone()));
        return Ok(());
    }
    msg_success!(Message::InboxUnsnoozed(key));
    Ok(())
}

/// Turns an inbox issue into a task and records that it was taken.
///
/// `take` used to create the task and dismiss the issue, which severed the
/// two: the key survived only inside the task's name, and the inbox forgot the
/// issue had ever been picked up. Now the task carries `issue_key`, and the
/// issue stays in the list wearing a `taken` badge.
///
/// With `start_on_take` the issue is also started in Jira; the take is said
/// first, because it happened whatever Jira answers next.
async fn take_issue(key: &str) -> Result<()> {
    match toast_apply::take(key, &MoveOptions::from_cli(None, None)).await? {
        Applied::Taken { started, tags } => {
            msg_success!(Message::InboxTaken(key.to_string()));
            if !tags.is_empty() {
                msg_info!(Message::TagsAddedToTask(tags.join(", ")));
            }
            match started {
                Some(Ok(moved)) => jira_bridge::report_move(key, Goal::Start, &moved),
                Some(Err(why)) => msg_warning!(Message::JiraWriteFailed(key.to_string(), why)),
                None => {}
            }
        }
        Applied::AlreadyTaken(name) => msg_info!(Message::InboxAlreadyTaken(key.to_string(), name)),
        _ => msg_error!(Message::InboxNotFound(key.to_string())),
    }
    Ok(())
}

/// Brings a dismissed issue back into the list.
///
/// The way back from a dismissal, by hand or by a rule. A rule that hid the
/// wrong issues is fixed in the config, and the issues it hid come back here -
/// without this, a mistaken rule would be a one-way door.
fn undismiss(key: Option<String>) -> Result<()> {
    let db = Inbox::new()?;
    let key = match key {
        Some(key) => key,
        None => {
            let hidden: Vec<InboxItem> = db
                .list(Include {
                    snoozed: true,
                    dismissed: true,
                    ..Include::default()
                })?
                .into_iter()
                .filter(|i| i.dismissed)
                .collect();
            if hidden.is_empty() {
                bail!("no issues are dismissed - `kasl inbox dismiss KEY` dismisses one");
            }
            pick::inbox_issue(&hidden, "Bring back which issue?")?
        }
    };

    if !db.set_dismissed(&key, false)? {
        msg_error!(Message::InboxNotFound(key));
        return Ok(());
    }
    msg_success!(Message::InboxUndismissed(key));
    Ok(())
}

/// Shows the inbox rules and what each matches now, applying them when asked.
///
/// Without `--apply` this is a dry run: what each rule would act on is shown
/// before it acts on anything, which is how a dismiss rule is checked before
/// it hides real work.
///
/// With `--json` the document carries what the table and the lines around it
/// say: the rules with what each matches, the ones skipped and why, and what
/// `--apply` changed.
fn rules(apply: bool, json: bool) -> Result<()> {
    let db = Inbox::new()?;
    let Some(config) = Config::read()?.inbox.filter(|inbox| !inbox.rules.is_empty()) else {
        if json {
            return output::print_json(&json::Rules {
                rules: Vec::new(),
                skipped: Vec::new(),
                applied: apply.then(Vec::new),
            });
        }
        msg_info!(Message::InboxRulesNone);
        return Ok(());
    };
    let book = RuleBook::of_inbox(&config, &db)?;
    if json {
        let applied = if apply && !book.is_empty() {
            Some(inbox_rules::apply_now(&db, &book)?)
        } else {
            apply.then(Vec::new)
        };
        let items = db.list(Include {
            snoozed: true,
            dismissed: true,
            ..Include::default()
        })?;
        return output::print_json(&json::Rules {
            rules: book.rules.iter().map(|rule| json::Rule::from(&inbox_rules::standing(rule, &items))).collect(),
            skipped: book
                .problems
                .iter()
                .map(|problem| json::RuleSkipped {
                    number: problem.number,
                    reason: problem.reason.clone(),
                })
                .collect(),
            applied: applied.map(|ruled| ruled.iter().map(json::RuleActed::from).collect()),
        });
    }
    for problem in &book.problems {
        msg_warning!(Message::InboxRuleSkipped(problem.number, problem.reason.clone()));
    }
    if book.is_empty() {
        return Ok(());
    }

    if apply {
        let ruled = inbox_rules::apply_now(&db, &book)?;
        for one in &ruled {
            msg_info!(Message::InboxRuleActed(inbox_rules::sentence(one)));
        }
        msg_success!(Message::InboxRulesApplied(ruled.len()));
    }

    let items = db.list(Include {
        snoozed: true,
        dismissed: true,
        ..Include::default()
    })?;
    let rows: Vec<inbox_rules::RuleStanding> = book.rules.iter().map(|rule| inbox_rules::standing(rule, &items)).collect();
    View::inbox_rules(&rows)?;
    let waiting: usize = rows.iter().map(|row| row.waiting).sum();
    if waiting > 0 && !apply {
        msg_info!(Message::InboxRulesNotApplied(waiting));
    }
    Ok(())
}

/// Says what the rules did during a sync, and which ones were skipped.
fn report_rules(ruled: &[inbox_rules::Ruled], problems: &[inbox_rules::RuleProblem]) {
    for problem in problems {
        msg_warning!(Message::InboxRuleSkipped(problem.number, problem.reason.clone()));
    }
    for one in ruled {
        msg_info!(Message::InboxRuleActed(inbox_rules::sentence(one)));
    }
}

/// One line over the list when a rule is being skipped.
///
/// The watcher has no terminal to say it on, and a rule that silently does
/// nothing looks exactly like a rule with nothing to match; the list is where
/// the person looks every day.
fn warn_skipped_rules() {
    let skipped = Config::read()
        .ok()
        .and_then(|c| c.inbox)
        .map(|inbox| RuleBook::compile(&inbox.rules, &[]).problems.len())
        .unwrap_or(0);
    if skipped > 0 {
        msg_warning!(Message::InboxRulesSkipped(skipped));
    }
}

/// Moves an issue into progress in Jira, by hand.
///
/// The same move `start_on_take` makes, for when it is off, or when the move
/// at take time could not happen. A move that does not arrive fails the
/// command: a script that asked for the issue to be started must be able to
/// tell that it was not.
async fn start_issue(key: &str, to: Option<String>) -> Result<()> {
    let moved = inbox_source::move_issue(key, Goal::Start, &MoveOptions::from_cli(to, None)).await?;
    finish_move(key, Goal::Start, moved)
}

/// Resolves an issue at its source, after the comment when one is given.
async fn close_issue(key: &str, comment: Option<String>, to: Option<String>, resolution: Option<String>) -> Result<()> {
    let moved = inbox_source::close_issue(key, comment.as_deref(), &MoveOptions::from_cli(to, resolution)).await?;
    finish_move(key, Goal::Close, moved)
}

/// Reports a move, and fails the command when the issue did not get there.
fn finish_move(key: &str, goal: Goal, moved: jira_bridge::Moved) -> Result<()> {
    if moved.arrived() {
        jira_bridge::report_move(key, goal, &moved);
        Ok(())
    } else {
        bail!("{}", moved.sentence(key, goal))
    }
}

/// Adds a comment to an issue at its source.
///
/// The text is asked for on a terminal when it is not given; an empty comment
/// is refused rather than sent, since the source would refuse it anyway,
/// less clearly.
async fn comment_issue(key: &str, text: Option<String>) -> Result<()> {
    let text = match text {
        Some(text) => text,
        None if is_interactive() => Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptJiraComment.to_string())
            .interact_text()?,
        None => bail!("comment text is required; pass TEXT outside an interactive terminal"),
    };
    let text = text.trim();
    if text.is_empty() {
        bail!("an empty comment is not a comment; nothing was sent");
    }
    inbox_source::comment_issue(key, text, crate::api::jira::SessionMode::Interactive).await?;
    msg_success!(Message::JiraCommented(key.to_string()));
    Ok(())
}

/// Offers to mark done the tasks whose issues this sync found resolved.
///
/// One question for all of them, every one ticked: the usual answer after a
/// resolution is "yes, I am done", and a run of separate prompts after a long
/// absence would be a quiz. Off a terminal the news is said instead, with the
/// command that offers the same choice later.
fn offer_resolved(resolved: &[ResolvedIssue]) -> Result<()> {
    if resolved.is_empty() {
        return Ok(());
    }
    let in_parens = |issue: &ResolvedIssue| issue.resolution.as_deref().map(|r| format!(" ({r})")).unwrap_or_default();
    if !is_interactive() {
        for issue in resolved {
            msg_info!(Message::JiraResolvedTaskOpen(issue.issue_key.clone(), in_parens(issue)));
        }
        return Ok(());
    }

    let labels: Vec<String> = resolved
        .iter()
        .map(|issue| format!("{} {}{}", issue.issue_key, issue.summary, in_parens(issue)))
        .collect();
    let ticked = vec![true; labels.len()];
    let chosen = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptMarkResolvedDone.to_string())
        .items(&labels)
        .defaults(&ticked)
        .interact()?;
    for index in chosen {
        let key = &resolved[index].issue_key;
        msg_success!(Message::ToastActionApplied(toast_apply::complete(key)?.summary(key)));
    }
    Ok(())
}
