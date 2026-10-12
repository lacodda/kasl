//! Inbox rules: what the inbox does by itself when an issue matches.
//!
//! A rule is a `when` and a `then`, written in `jira_inbox.rules`:
//!
//! ```json
//! { "name": "on fire", "when": { "priority": "High+" }, "then": { "pin": true, "urgent": true } }
//! { "when": { "type": ["Epic", "Sub-task"] }, "then": { "dismiss": true } }
//! { "when": { "project": "PAY" }, "then": { "tag": "payments" } }
//! ```
//!
//! `when` speaks the vocabulary of the list's filters - `priority` (with `+`
//! for "and above"), `min_score`, `status`, `type`, `project` - and compiles to
//! the same [`InboxFilter`], so a rule pins exactly what `kasl inbox list` with
//! the same cuts would show.
//!
//! **When a rule acts.** Pin, urgent and dismiss act on an edge, not a level:
//! when an issue arrives already matching, or when a change makes it match. A
//! rule re-applied on every poll would fight the person - unpin an issue the
//! rule pinned, and five minutes later it is pinned again. On an edge, the
//! person's word stands until the issue leaves the rule and comes back into
//! it, which is news. Tags act when the issue is taken, on every take path.
//! The issues already in the inbox when a rule is written are reached by
//! `kasl inbox rules --apply`, once and on purpose.
//!
//! **When rules disagree.** Every matching rule acts. Dismissal beats pin and
//! urgent: an issue a rule hides is not also pinned and announced. A rule never
//! dismisses a taken issue - that is work in hand, and hiding it would hide the
//! work - and no rule touches an issue that is already dismissed.

use crate::db::jira_inbox::{Include, JiraInbox, JiraInboxItem, RuledMark};
use crate::db::tags::Tags;
use crate::db::tasks::Tasks;
use crate::libs::config::{Config, JiraInboxConfig};
use crate::libs::inbox_filter::{InboxFilter, Reason, priority_cut_in};
use crate::libs::task::TaskFilter;
use anyhow::Result;
use chrono::Local;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::HashMap;

/// When a rule applies. Every given condition must hold; a list means any of it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleWhen {
    /// A priority name, with `+` for that priority and everything more urgent.
    #[serde(default)]
    pub priority: Option<String>,
    /// The ranking field (e.g. Scoring) at least this.
    #[serde(default)]
    pub min_score: Option<f64>,
    /// Status names or ids.
    #[serde(default, deserialize_with = "one_or_many")]
    pub status: Vec<String>,
    /// Issue types.
    #[serde(default, rename = "type", deserialize_with = "one_or_many")]
    pub types: Vec<String>,
    /// Projects (in Jira, the prefix of the key).
    #[serde(default, rename = "project", deserialize_with = "one_or_many")]
    pub projects: Vec<String>,
}

impl RuleWhen {
    /// Whether no condition is given, so the rule would match every issue.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The conditions in words, as the label of a rule without a name.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(priority) = &self.priority {
            parts.push(format!("priority {priority}"));
        }
        if let Some(score) = self.min_score {
            parts.push(format!("score ≥ {score}"));
        }
        for (what, names) in [("status", &self.status), ("type", &self.types), ("project", &self.projects)] {
            if !names.is_empty() {
                parts.push(format!("{what} {}", names.join(" or ")));
            }
        }
        if parts.is_empty() { "every issue".to_string() } else { parts.join(", ") }
    }
}

/// What a rule does to a matching issue.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleThen {
    /// Pin it, so it leads the list.
    #[serde(default)]
    pub pin: bool,
    /// Toast it as urgent: a toast that stays on screen until acted on.
    #[serde(default)]
    pub urgent: bool,
    /// Hide it from the list, as `kasl inbox dismiss` does.
    #[serde(default)]
    pub dismiss: bool,
    /// Tags for the task taken from it.
    #[serde(default, deserialize_with = "one_or_many")]
    pub tag: Vec<String>,
}

impl RuleThen {
    /// The actions in words.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.pin {
            parts.push("pin".to_string());
        }
        if self.urgent {
            parts.push("urgent toast".to_string());
        }
        if self.dismiss {
            parts.push("dismiss".to_string());
        }
        if !self.tag.is_empty() {
            parts.push(format!("tag {}", self.tag.join(", ")));
        }
        parts.join(", ")
    }
}

/// One rule as written.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxRule {
    /// What the rule is called in messages, toasts and `show --why`; without
    /// one, its conditions in words.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub when: RuleWhen,
    pub then: RuleThen,
}

impl InboxRule {
    /// Reads one rule, refusing anything that would act other than as written.
    ///
    /// Unknown keys are refused rather than ignored: `prority` silently
    /// dropped would leave an empty `when`, which matches every issue - and
    /// with `dismiss` that empties the inbox.
    pub fn parse(value: &Value) -> std::result::Result<InboxRule, String> {
        let rule: InboxRule = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        let then = &rule.then;
        if !then.pin && !then.urgent && !then.dismiss && then.tag.is_empty() {
            return Err("it does nothing: `then` needs pin, urgent, dismiss or tag".to_string());
        }
        if then.dismiss && (then.pin || then.urgent) {
            return Err("it both dismisses and pins or announces; a hidden issue is neither - split it into two rules".to_string());
        }
        if then.dismiss && rule.when.is_empty() {
            return Err("it would dismiss every issue; give `when` at least one condition".to_string());
        }
        if then.tag.iter().any(|tag| tag.trim().is_empty()) {
            return Err("a tag name cannot be empty".to_string());
        }
        if rule.when.priority.as_deref().is_some_and(|p| p.trim_end_matches('+').trim().is_empty()) {
            return Err("`priority` names no priority".to_string());
        }
        Ok(rule)
    }

    /// The name, or the conditions in words.
    pub fn label(&self) -> String {
        match self.name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => self.when.describe(),
        }
    }
}

/// Accepts one name or a list of names, as the config may write either.
fn one_or_many<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
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

/// A rule ready to match, with where it sits in the config.
#[derive(Debug, Clone)]
pub struct CompiledRule {
    /// Position in `jira_inbox.rules`, counting from 1.
    pub number: usize,
    pub rule: InboxRule,
    pub label: String,
    filter: InboxFilter,
    /// Why the rule can match nothing yet: a priority the inbox has never seen.
    pub dormant: Option<String>,
}

impl CompiledRule {
    /// Whether the issue meets every condition.
    pub fn matches(&self, item: &JiraInboxItem) -> bool {
        // Rules carry no time windows, so the moment passed is never read.
        self.dormant.is_none() && self.filter.keeps(item, item.last_seen)
    }
}

/// A rule that could not be read, and why; it is skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleProblem {
    /// Position in `jira_inbox.rules`, counting from 1.
    pub number: usize,
    pub reason: String,
}

/// The rules of the inbox, read and compiled.
#[derive(Debug, Clone, Default)]
pub struct RuleBook {
    pub rules: Vec<CompiledRule>,
    pub problems: Vec<RuleProblem>,
}

/// What the rules say about one issue: the rule behind each action.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Verdict {
    pub pin: Option<String>,
    pub urgent: Option<String>,
    pub dismiss: Option<String>,
    pub tags: Vec<String>,
}

impl RuleBook {
    /// Reads the rules, resolving priority names against `priorities`.
    ///
    /// Priority names are the instance's own, so they are looked up among the
    /// priorities the inbox has seen. A name not among them leaves its rule
    /// dormant - matching nothing - rather than failing: "Highest" may simply
    /// not have been assigned to anyone yet.
    pub fn compile(values: &[Value], priorities: &[(String, i32)]) -> RuleBook {
        let mut book = RuleBook::default();
        for (index, value) in values.iter().enumerate() {
            let number = index + 1;
            let rule = match InboxRule::parse(value) {
                Ok(rule) => rule,
                Err(reason) => {
                    book.problems.push(RuleProblem { number, reason });
                    continue;
                }
            };
            let (priority, dormant) = match rule.when.priority.as_deref().map(|name| priority_cut_in(priorities, name)) {
                Some(Ok(cut)) => (Some(cut), None),
                Some(Err(why)) => (None, Some(why.to_string())),
                None => (None, None),
            };
            let filter = InboxFilter {
                priority,
                min_score: rule.when.min_score,
                status: rule.when.status.clone(),
                types: rule.when.types.clone(),
                projects: rule.when.projects.clone(),
                ..InboxFilter::default()
            };
            book.rules.push(CompiledRule {
                number,
                label: rule.label(),
                rule,
                filter,
                dormant,
            });
        }
        book
    }

    /// The rules of the configured inbox, against the priorities it has seen.
    pub fn of_inbox(config: &JiraInboxConfig, db: &JiraInbox) -> Result<RuleBook> {
        if config.rules.is_empty() {
            return Ok(RuleBook::default());
        }
        Ok(Self::compile(&config.rules, &db.priorities()?))
    }

    /// The rules as the config on disk has them; none when it cannot be read.
    pub fn from_config(db: &JiraInbox) -> Result<RuleBook> {
        match Config::read().ok().and_then(|c| c.jira_inbox) {
            Some(inbox) => Self::of_inbox(&inbox, db),
            None => Ok(RuleBook::default()),
        }
    }

    /// Whether there is nothing to apply.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// What every rule the issue matches says about it now.
    pub fn verdict(&self, item: &JiraInboxItem) -> Verdict {
        self.collect(|rule| rule.matches(item))
    }

    /// What the rules the issue has just come to match say about it.
    ///
    /// A rule counts when the issue matches it now and did not before - or
    /// there was no before, because the issue is new.
    pub fn entering(&self, before: Option<&JiraInboxItem>, now: &JiraInboxItem) -> Verdict {
        self.collect(|rule| rule.matches(now) && !before.is_some_and(|old| rule.matches(old)))
    }

    fn collect(&self, counts: impl Fn(&CompiledRule) -> bool) -> Verdict {
        let mut verdict = Verdict::default();
        for rule in self.rules.iter().filter(|rule| counts(rule)) {
            let then = &rule.rule.then;
            if then.pin && verdict.pin.is_none() {
                verdict.pin = Some(rule.label.clone());
            }
            if then.urgent && verdict.urgent.is_none() {
                verdict.urgent = Some(rule.label.clone());
            }
            if then.dismiss && verdict.dismiss.is_none() {
                verdict.dismiss = Some(rule.label.clone());
            }
            for tag in &then.tag {
                let tag = tag.trim().to_string();
                if !verdict.tags.contains(&tag) {
                    verdict.tags.push(tag);
                }
            }
        }
        verdict
    }
}

/// What the rules did to one issue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ruled {
    pub issue_key: String,
    /// The rule that pinned it.
    pub pinned: Option<String>,
    /// The rule that dismissed it.
    pub dismissed: Option<String>,
    /// The rule that asks for an urgent toast about it.
    pub urgent: Option<String>,
    /// Whether an urgent rule woke it from a snooze.
    pub woke: bool,
    /// Tags put on the task taken from it.
    pub tagged: Vec<String>,
}

impl Ruled {
    fn new(key: &str) -> Self {
        Ruled {
            issue_key: key.to_string(),
            ..Default::default()
        }
    }

    fn acted(&self) -> bool {
        self.pinned.is_some() || self.dismissed.is_some() || self.urgent.is_some() || !self.tagged.is_empty()
    }
}

/// Applies the rules each issue of a poll has just come to match.
///
/// `before` is the inbox as it stood before the poll was written, keyed by
/// issue; an issue missing from it is new. Returns what the rules did, for
/// the toasts and the sync's report.
pub fn apply_entering(db: &JiraInbox, book: &RuleBook, before: &HashMap<String, JiraInboxItem>, keys: &[String]) -> Result<Vec<Ruled>> {
    let now = Local::now().naive_local();
    let mut done = Vec::new();
    for key in keys {
        let Some(item) = db.get_by_key(key)? else { continue };
        if item.dismissed {
            continue;
        }
        let verdict = book.entering(before.get(key), &item);
        let mut ruled = Ruled::new(key);

        match verdict.dismiss.filter(|_| item.taken_at.is_none()) {
            Some(rule) => {
                db.set_ruled(key, RuledMark::Dismiss, &rule)?;
                ruled.dismissed = Some(rule);
            }
            None => {
                if let Some(rule) = verdict.pin
                    && !item.pinned
                {
                    db.set_ruled(key, RuledMark::Pin, &rule)?;
                    ruled.pinned = Some(rule);
                }
                if let Some(rule) = verdict.urgent {
                    // The snooze was "not now", judged on what the issue was
                    // then; a toast calling it urgent about an issue the list
                    // still hides would contradict itself.
                    if item.snoozed_until.is_some_and(|until| until > now) {
                        db.set_snoozed(key, None)?;
                        ruled.woke = true;
                    }
                    ruled.urgent = Some(rule);
                }
            }
        }
        if ruled.acted() {
            done.push(ruled);
        }
    }
    Ok(done)
}

/// Applies the rules to the inbox as it stands, as if every issue had just arrived.
///
/// For the issues that were already there when a rule was written: an edge
/// that happened before the rule existed is never seen again. Pins and
/// dismisses, and tags the tasks of issues already taken; no toasts - none of
/// this is news. Asleep issues count, gone ones do not.
pub fn apply_now(db: &JiraInbox, book: &RuleBook) -> Result<Vec<Ruled>> {
    let items = db.list(Include {
        snoozed: true,
        ..Include::default()
    })?;
    let mut done = Vec::new();
    for item in items {
        let verdict = book.verdict(&item);
        let mut ruled = Ruled::new(&item.issue_key);
        match verdict.dismiss.filter(|_| item.taken_at.is_none()) {
            Some(rule) => {
                db.set_ruled(&item.issue_key, RuledMark::Dismiss, &rule)?;
                ruled.dismissed = Some(rule);
            }
            None => {
                if let Some(rule) = verdict.pin
                    && !item.pinned
                {
                    db.set_ruled(&item.issue_key, RuledMark::Pin, &rule)?;
                    ruled.pinned = Some(rule);
                }
            }
        }
        if item.taken_at.is_some() && !verdict.tags.is_empty() {
            ruled.tagged = tag_task(&item.issue_key, &verdict.tags)?;
        }
        if ruled.acted() {
            done.push(ruled);
        }
    }
    Ok(done)
}

/// The tags the rules give the task taken from this issue.
pub fn tags_for(item: &JiraInboxItem) -> Vec<String> {
    match JiraInbox::new().and_then(|db| RuleBook::from_config(&db)) {
        Ok(book) => book.verdict(item).tags,
        Err(_) => Vec::new(),
    }
}

/// Puts tags on the task taken from `key`, and returns the ones it did not have.
///
/// The tags go on the task's first row - the one the take made - which is
/// where `kasl task add --tags` puts them too. Tags already there are left
/// alone, so applying the rules twice changes nothing the second time.
pub fn tag_task(key: &str, names: &[String]) -> Result<Vec<String>> {
    let rows = Tasks::new()?.fetch(TaskFilter::ByJiraKey(key.to_string()))?;
    let Some(task_id) = rows
        .iter()
        .find(|row| row.id.is_some() && row.id == row.task_id)
        .or_else(|| rows.iter().min_by_key(|row| row.id))
        .and_then(|row| row.id)
    else {
        return Ok(Vec::new());
    };

    let mut tags = Tags::new()?;
    let had: Vec<String> = tags.get_tags_by_task(task_id)?.into_iter().map(|tag| tag.name).collect();
    let new: Vec<String> = names.iter().filter(|name| !had.contains(name)).cloned().collect();
    for id in tags.get_or_create_tags(&new)? {
        tags.add_tag_to_task(task_id, id)?;
    }
    Ok(new)
}

/// What the rules did to one issue, in one line.
pub fn sentence(ruled: &Ruled) -> String {
    let mut parts = Vec::new();
    if let Some(rule) = &ruled.dismissed {
        parts.push(format!("dismissed by '{rule}'"));
    }
    if let Some(rule) = &ruled.pinned {
        parts.push(format!("pinned by '{rule}'"));
    }
    if let Some(rule) = &ruled.urgent {
        parts.push(format!("urgent by '{rule}'"));
    }
    if ruled.woke {
        parts.push("woken from its snooze".to_string());
    }
    if !ruled.tagged.is_empty() {
        parts.push(format!("its task tagged {}", ruled.tagged.join(", ")));
    }
    format!("{}: {}", ruled.issue_key, parts.join("; "))
}

/// The rules an issue matches now, as lines of `kasl inbox show --why`.
pub fn explain(book: &RuleBook, item: &JiraInboxItem) -> Vec<Reason> {
    book.rules
        .iter()
        .filter(|rule| rule.matches(item))
        .map(|rule| Reason {
            what: "rule",
            value: rule.label.clone(),
            because: format!(
                "rule {} matches it now and says: {}; a rule acts when an issue arrives or comes to match it",
                rule.number,
                rule.rule.then.describe()
            ),
        })
        .collect()
}

/// One rule as `kasl inbox rules` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleStanding {
    pub number: usize,
    pub name: Option<String>,
    pub when: String,
    pub then: String,
    /// What it matches in the inbox now, in words.
    pub now: String,
    /// Matching issues its pin or dismissal has not reached - what `--apply` would change.
    pub waiting: usize,
}

/// Where one rule stands against the inbox: what it matches, and what it has not reached.
///
/// `items` is the present inbox, asleep and dismissed issues included - a
/// dismiss rule's work is exactly the issues the ordinary list no longer shows.
pub fn standing(rule: &CompiledRule, items: &[JiraInboxItem]) -> RuleStanding {
    let mut standing = RuleStanding {
        number: rule.number,
        name: rule.rule.name.clone().filter(|name| !name.trim().is_empty()),
        when: rule.rule.when.describe(),
        then: rule.rule.then.describe(),
        now: String::new(),
        waiting: 0,
    };
    if let Some(why) = &rule.dormant {
        standing.now = format!("matches nothing yet: {why}");
        return standing;
    }

    let matching: Vec<&JiraInboxItem> = items.iter().filter(|item| rule.matches(item)).collect();
    let count = |keep: fn(&JiraInboxItem) -> bool| matching.iter().filter(|item| keep(item)).count();
    let mut parts = vec![format!("{} match", matching.len())];
    let then = &rule.rule.then;
    // Zero counts are left out: "0 pinned" next to "1 not yet pinned" says
    // the same thing twice.
    let mut say = |n: usize, what: &str| {
        if n > 0 {
            parts.push(format!("{n} {what}"));
        }
    };
    if then.dismiss {
        say(count(|i| i.dismissed), "dismissed");
        let not_yet = count(|i| !i.dismissed && i.taken_at.is_none());
        say(not_yet, "not yet dismissed");
        say(count(|i| !i.dismissed && i.taken_at.is_some()), "kept as taken");
        standing.waiting += not_yet;
    }
    if then.pin {
        say(count(|i| !i.dismissed && i.pinned), "pinned");
        let not_yet = count(|i| !i.dismissed && !i.pinned);
        say(not_yet, "not yet pinned");
        standing.waiting += not_yet;
    }
    if !then.tag.is_empty() {
        say(count(|i| i.taken_at.is_some()), "taken");
    }
    standing.now = parts.join(", ");
    standing
}
