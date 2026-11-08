//! Slicing the inbox: which issues to show, and in what order.
//!
//! An inbox of two hundred open issues is not a list anyone reads top to
//! bottom; it is a pile that gets useful only once it can be cut - what
//! arrived this week, what scores above five, what is High or worse. The cuts
//! live here as plain functions over already-loaded items: the table is small
//! enough that SQL buys nothing, and a function is what the tests can hold.
//!
//! The same cut is what an inbox rule's `when` compiles to
//! ([`crate::libs::inbox_rules`]), so "the issues `--priority High+` lists"
//! and "the issues a `priority: High+` rule pins" cannot come apart.

use crate::db::inbox::InboxItem;
use anyhow::{Result, bail};
use chrono::{Duration, NaiveDateTime};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// A named cut of the list, as `inbox.presets` writes it: the flags of
/// `kasl inbox list`, under their own names.
///
/// `kasl inbox --preset urgent` is `--priority High+ --since 7d` when the
/// config says `"urgent": {"priority": "High+", "since": "7d"}`; a flag given
/// beside the preset wins over the preset's field. Kept in the config as
/// written and read here when a command names it, so a misspelt preset fails
/// the command that asks for it rather than the whole file.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InboxPreset {
    /// Only issues first seen within a window: 1d, 7d, 12h, 2w.
    #[serde(default)]
    pub since: Option<String>,
    /// Only issues discovered in the last day.
    #[serde(default)]
    pub new: bool,
    /// Only issues that visibly changed within a window: 1d, 7d, 12h, 2w.
    #[serde(default)]
    pub changed: Option<String>,
    /// Only issues whose ranking field is at least this.
    #[serde(default)]
    pub min_score: Option<f64>,
    /// A priority name, with `+` for that priority and everything more urgent.
    #[serde(default)]
    pub priority: Option<String>,
    /// Status names, any of them.
    #[serde(default, deserialize_with = "one_or_many")]
    #[schemars(with = "OneOrMany")]
    pub status: Vec<String>,
    /// Issue types, any of them.
    #[serde(default, rename = "type", deserialize_with = "one_or_many")]
    #[schemars(with = "OneOrMany")]
    pub types: Vec<String>,
    /// Projects, any of them.
    #[serde(default, rename = "project", deserialize_with = "one_or_many")]
    #[schemars(with = "OneOrMany")]
    pub projects: Vec<String>,
    /// Order of the list; pinned issues lead whatever the order.
    #[serde(default)]
    pub sort: Option<InboxSort>,
}

impl InboxPreset {
    /// Reads one preset, refusing anything that would cut other than as written.
    ///
    /// Unknown keys are refused: `prority` dropped silently would leave the
    /// preset listing everything while its name promises a slice.
    pub fn parse(value: &Value) -> std::result::Result<InboxPreset, String> {
        let preset: InboxPreset = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        preset.check()?;
        Ok(preset)
    }

    /// What the schema cannot say about a preset that reads.
    pub fn check(&self) -> std::result::Result<(), String> {
        if self.new && self.since.is_some() {
            return Err("`new` is the last day's `since`; give one of them".to_string());
        }
        for window in [&self.since, &self.changed].into_iter().flatten() {
            parse_window(window).map_err(|e| e.to_string())?;
        }
        if self.priority.as_deref().is_some_and(|p| p.trim_end_matches('+').trim().is_empty()) {
            return Err("`priority` names no priority".to_string());
        }
        if self.min_score.is_some_and(|score| !score.is_finite()) {
            return Err("`min_score` is not a number".to_string());
        }
        Ok(())
    }
}

/// One name, or a list of names meaning any of them.
//
// Only the schema's view of such a field: the fields themselves are
// `Vec<String>`, read by [`one_or_many`].
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
pub(crate) enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

/// Accepts one name or a list of names, as the config may write either.
pub(crate) fn one_or_many<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match Value::deserialize(deserializer)? {
        Value::String(one) => Ok(vec![one]),
        Value::Array(many) => many
            .into_iter()
            .map(|v| match v {
                Value::String(s) => Ok(s),
                other => Err(serde::de::Error::custom(format!("expected a name, found {other}"))),
            })
            .collect(),
        other => Err(serde::de::Error::custom(format!("expected a name or a list of names, found {other}"))),
    }
}

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
    /// Status names or ids, any of them; matched without regard to case.
    pub status: Vec<String>,
    /// Issue types, any of them; matched without regard to case.
    pub types: Vec<String>,
    /// Projects, any of them; matched without regard to case.
    pub projects: Vec<String>,
}

impl InboxFilter {
    /// True when nothing is set and the list would come back whole.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether one issue passes every set cut.
    pub fn keeps(&self, item: &InboxItem, now: NaiveDateTime) -> bool {
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
        if !self.status.is_empty()
            && !self
                .status
                .iter()
                .any(|status| same_word(&item.status_name, status) || item.status_id.as_deref().is_some_and(|id| same_word(id, status)))
        {
            return false;
        }
        if !one_of(item.issue_type.as_deref(), &self.types) {
            return false;
        }
        if !one_of(item.project.as_deref(), &self.projects) {
            return false;
        }
        true
    }

    /// Keeps the issues that pass, in their incoming order.
    pub fn apply(&self, items: Vec<InboxItem>, now: NaiveDateTime) -> Vec<InboxItem> {
        items.into_iter().filter(|item| self.keeps(item, now)).collect()
    }
}

/// The cut behind a priority name, against priorities known by name and rank.
///
/// Priority names belong to the Jira instance (a Russian Jira says
/// «Высокий», not "High"), so they are looked up among the priorities the
/// inbox has seen ([`crate::db::inbox::Inbox::priorities`]) rather
/// than in a table of English defaults. A trailing `+` widens the cut to
/// that priority and everything more urgent.
pub fn priority_cut_in(known: &[(String, i32)], wanted: &str) -> Result<PriorityCut> {
    let (name, and_above) = match wanted.strip_suffix('+') {
        Some(name) => (name.trim(), true),
        None => (wanted.trim(), false),
    };
    let rank = known.iter().filter(|(known, _)| same_word(known, name)).map(|(_, rank)| *rank).min();
    let Some(rank) = rank else {
        let mut names: Vec<&str> = known.iter().map(|(name, _)| name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        if names.is_empty() {
            bail!("no priority named '{name}' in the inbox, and no issue carries a priority yet");
        }
        bail!("no priority named '{name}' in the inbox; known: {}", names.join(", "));
    };
    Ok(if and_above { PriorityCut::AtLeast(rank) } else { PriorityCut::Exactly(rank) })
}

/// Whether two names are the same word, whatever the case.
///
/// Full Unicode lowercasing, not ASCII: the names are the instance's own, and
/// a Russian Jira's type typed in lower case is still the same type.
fn same_word(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Whether a field passes an any-of cut: an empty cut passes everything, and
/// an empty field passes no cut.
fn one_of(value: Option<&str>, wanted: &[String]) -> bool {
    wanted.is_empty() || value.is_some_and(|value| wanted.iter().any(|w| same_word(value, w)))
}

/// What the list is ordered by. Pinned issues lead, and gone then dismissed issues trail, whatever the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
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
pub fn sort(items: &mut [InboxItem], by: InboxSort) {
    items.sort_by(|a, b| {
        let frame = a
            .gone_at
            .is_some()
            .cmp(&b.gone_at.is_some())
            .then_with(|| a.dismissed.cmp(&b.dismissed))
            .then_with(|| b.pinned.cmp(&a.pinned));
        frame.then_with(|| match by {
            InboxSort::Score => std::cmp::Ordering::Equal,
            InboxSort::Priority => a.priority_rank.cmp(&b.priority_rank),
            InboxSort::New => b.first_seen.cmp(&a.first_seen),
            InboxSort::Changed => b.changed_at.cmp(&a.changed_at),
        })
    });
}

/// One line of the answer to "why is this issue here?".
#[derive(Debug, Clone, PartialEq)]
pub struct Reason {
    /// What is being explained: `score`, `priority`, `pinned`.
    pub what: &'static str,
    /// The value as the row carries it.
    pub value: String,
    /// Where it comes from and what it does to the order.
    pub because: String,
}

/// Explains an issue's place in the list.
///
/// Deliberately not a decomposed score. kasl does not compute importance: the
/// score is a number read straight out of a Jira field the user named, and the
/// priority rank is Jira's own priority id. Inventing a local formula to
/// decompose - "High +3, due tomorrow +2" - would put a second, disagreeing
/// answer next to Jira's about which issue matters, and the user would have no
/// way to tell which one is real. So this explains what the order is actually
/// made of and where each part came from.
///
/// `score_label` is what the configured ranking field is called (typically
/// `Scoring`); naming it is most of the answer, because the number means
/// nothing without knowing which field it is.
pub fn explain(item: &InboxItem, score_label: Option<&str>, now: NaiveDateTime) -> Vec<Reason> {
    let mut out = Vec::new();
    let label = score_label.unwrap_or("the ranking field");

    match item.sort_value {
        Some(score) => out.push(Reason {
            what: "score",
            value: format!("{score}"),
            because: format!("read from {label} in Jira; the list is ordered by it, highest first"),
        }),
        None => out.push(Reason {
            what: "score",
            value: "—".to_string(),
            because: format!("{label} is empty on this issue in Jira; issues without a score sort below those with one"),
        }),
    }

    out.push(Reason {
        what: "priority",
        value: item.priority.clone().unwrap_or_else(|| "—".to_string()),
        because: match item.priority.as_deref() {
            Some(_) => format!(
                "Jira priority id {}, which breaks ties on equal scores - lower is more urgent",
                item.priority_rank
            ),
            None => "no priority set in Jira, so this sorts last among equal scores".to_string(),
        },
    });

    if item.pinned && !item.dismissed {
        out.push(Reason {
            what: "pinned",
            value: "yes".to_string(),
            because: match &item.ruled_by {
                Some(rule) => format!("pinned by the rule '{rule}'; pinned issues lead the list whatever the order, and `kasl inbox unpin` takes it back"),
                None => "pinned by hand; pinned issues lead the list whatever the order".to_string(),
            },
        });
    }

    if item.dismissed {
        out.push(Reason {
            what: "dismissed",
            value: "yes".to_string(),
            because: match &item.ruled_by {
                Some(rule) => format!("hidden by the rule '{rule}'; only `--dismissed` lists it, and `kasl inbox undismiss` brings it back"),
                None => "dismissed by hand; only `--dismissed` lists it, and `kasl inbox undismiss` brings it back".to_string(),
            },
        });
    }

    if let Some(until) = item.snoozed_until
        && until > now
    {
        out.push(Reason {
            what: "snoozed",
            value: until.format("%b %-d %H:%M").to_string(),
            because: "asleep until then, so it is out of the list and out of the waiting count".to_string(),
        });
    }

    if let Some(taken) = item.taken_at {
        out.push(Reason {
            what: "taken",
            value: taken.format("%b %-d").to_string(),
            because: "already a task; it stays in the list so what is in hand stays visible".to_string(),
        });
    }

    if let Some(gone) = item.gone_at {
        let because = match item.left_reason {
            Some(crate::db::inbox::LeftReason::Resolved) => "Jira resolved it, so it left the poll; only `--all` shows it",
            Some(crate::db::inbox::LeftReason::Moved) => "left the poll unresolved - reassigned or out of reach; only `--all` shows it",
            None => "stopped appearing in the Jira poll - closed or reassigned; only `--all` shows it",
        };
        out.push(Reason {
            what: "gone",
            value: gone.format("%b %-d").to_string(),
            because: because.to_string(),
        });
    }

    if let (Some(change), Some(at)) = (&item.last_change, item.changed_at) {
        out.push(Reason {
            what: "changed",
            value: change.clone(),
            because: format!("seen on {}; the badge fades after a day", at.format("%b %-d %H:%M")),
        });
    }

    out
}
