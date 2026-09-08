//! Slicing the inbox: which issues to show, and in what order.
//!
//! An inbox of two hundred open issues is not a list anyone reads top to
//! bottom; it is a pile that gets useful only once it can be cut - what
//! arrived this week, what scores above five, what is High or worse. The cuts
//! live here as plain functions over already-loaded items: the table is small
//! enough that SQL buys nothing, and a function is what the tests can hold.

use crate::db::jira_inbox::JiraInboxItem;
use anyhow::{Result, bail};
use chrono::{Duration, NaiveDateTime};

/// A time window such as `7d`, `12h`, `2w`, or a bare number of days.
///
/// The unit is one letter and the number is whole: this is typed on the
/// command line by someone who wants "this week", not a duration grammar.
pub fn parse_window(text: &str) -> Result<Duration> {
    let text = text.trim();
    let (digits, unit) = match text.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((at, _)) => text.split_at(at),
        None => (text, "d"),
    };
    let amount: i64 = match digits.parse() {
        Ok(n) if n > 0 => n,
        _ => bail!("'{text}' is not a window; try 1d, 7d, 12h or 2w"),
    };
    match unit {
        "h" => Ok(Duration::hours(amount)),
        "d" => Ok(Duration::days(amount)),
        "w" => Ok(Duration::weeks(amount)),
        _ => bail!("'{text}' is not a window; the unit is h, d or w"),
    }
}

/// A cut by priority rank. Lower rank is more urgent in Jira (`1` = Highest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriorityCut {
    /// That priority only.
    Exactly(i32),
    /// That priority and everything more urgent - the `NAME+` form.
    AtLeast(i32),
}

impl PriorityCut {
    fn keeps(self, rank: i32) -> bool {
        match self {
            PriorityCut::Exactly(wanted) => rank == wanted,
            PriorityCut::AtLeast(wanted) => rank <= wanted,
        }
    }
}

/// What the inbox is cut by. Every set field must hold; unset fields pass everything.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct InboxFilter {
    /// First seen no earlier than this long ago.
    pub since: Option<Duration>,
    /// Visibly changed no earlier than this long ago.
    pub changed: Option<Duration>,
    /// Ranking field (e.g. Scoring) at least this; issues without a score fail.
    pub min_score: Option<f64>,
    /// Priority, exact or "and above".
    pub priority: Option<PriorityCut>,
    /// Status name, matched without regard to case.
    pub status: Option<String>,
}

impl InboxFilter {
    /// True when nothing is set and the list would come back whole.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether one issue passes every set cut.
    pub fn keeps(&self, item: &JiraInboxItem, now: NaiveDateTime) -> bool {
        let within = |at: Option<NaiveDateTime>, window: Duration| at.is_some_and(|at| now.signed_duration_since(at) <= window);
        if let Some(window) = self.since
            && !within(Some(item.first_seen), window)
        {
            return false;
        }
        if let Some(window) = self.changed
            && !within(item.changed_at, window)
        {
            return false;
        }
        if let Some(floor) = self.min_score
            && !item.sort_value.is_some_and(|score| score >= floor)
        {
            return false;
        }
        if let Some(cut) = self.priority
            && !cut.keeps(item.priority_rank)
        {
            return false;
        }
        if let Some(status) = &self.status
            && !item.status_name.eq_ignore_ascii_case(status)
            && !item.status_id.as_deref().is_some_and(|id| id.eq_ignore_ascii_case(status))
        {
            return false;
        }
        true
    }

    /// Keeps the issues that pass, in their incoming order.
    pub fn apply(&self, items: Vec<JiraInboxItem>, now: NaiveDateTime) -> Vec<JiraInboxItem> {
        items.into_iter().filter(|item| self.keeps(item, now)).collect()
    }
}

/// The cut behind a priority name, as the inbox has seen it.
///
/// Priority names belong to the Jira instance (a Russian Jira says
/// «Высокий», not "High"), so they are looked up in the issues at hand rather
/// than in a table of English defaults. A trailing `+` widens the cut to
/// that priority and everything more urgent.
pub fn priority_cut_named(items: &[JiraInboxItem], wanted: &str) -> Result<PriorityCut> {
    let (name, and_above) = match wanted.strip_suffix('+') {
        Some(name) => (name.trim(), true),
        None => (wanted.trim(), false),
    };
    let rank = items
        .iter()
        .filter(|item| item.priority.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(name)))
        .map(|item| item.priority_rank)
        .min();
    let Some(rank) = rank else {
        let mut known: Vec<&str> = items.iter().filter_map(|item| item.priority.as_deref()).collect();
        known.sort_unstable();
        known.dedup();
        if known.is_empty() {
            bail!("no priority named '{name}' in the inbox, and no issue carries a priority yet");
        }
        bail!("no priority named '{name}' in the inbox; known: {}", known.join(", "));
    };
    Ok(if and_above { PriorityCut::AtLeast(rank) } else { PriorityCut::Exactly(rank) })
}

/// What the list is ordered by. Pinned issues lead and gone issues trail whatever the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InboxSort {
    /// Ranking field descending, then priority - the inbox's own order.
    #[default]
    Score,
    /// Most urgent first, then score.
    Priority,
    /// Most recently discovered first.
    New,
    /// Most recently changed first; issues never changed trail.
    Changed,
}

/// Reorders in place. Stable, so equal keys keep the inbox's own order.
pub fn sort(items: &mut [JiraInboxItem], by: InboxSort) {
    items.sort_by(|a, b| {
        let frame = a.gone_at.is_some().cmp(&b.gone_at.is_some()).then_with(|| b.pinned.cmp(&a.pinned));
        frame.then_with(|| match by {
            InboxSort::Score => std::cmp::Ordering::Equal,
            InboxSort::Priority => a.priority_rank.cmp(&b.priority_rank),
            InboxSort::New => b.first_seen.cmp(&a.first_seen),
            InboxSort::Changed => b.changed_at.cmp(&a.changed_at),
        })
    });
}
