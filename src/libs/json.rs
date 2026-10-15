//! The documents `--json` prints: one contract for scripts, status bars and
//! the MCP server that reads the same functions.
//!
//! Rules every document keeps, so a reader learns them once:
//!
//! - The top level is an object, never a bare list, so a field can be added
//!   without breaking anyone who reads the ones already there.
//! - Every key is always present. A value that does not exist is `null`, an
//!   empty list is `[]` - a reader never has to tell "absent" from "missing".
//! - Moments are local time, `YYYY-MM-DDTHH:MM:SS` (see
//!   [`crate::libs::output::moment`]); dates are `YYYY-MM-DD`.
//! - Durations are whole seconds, in a field whose name ends in `_seconds`.
//! - Percentages are numbers from 0 to 100, rounded to one decimal.
//!
//! The shapes are built from the domain types here rather than derived on
//! them: a table can change shape without the contract noticing, and the
//! contract changes only on purpose - with its page in the docs.

use crate::db::jira_inbox::{JiraInboxItem, LeftReason};
use crate::libs::output::moment;
use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use serde::Serialize;

/// A percentage rounded to one decimal, the precision kasl shows it at.
pub fn percent(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// Whole seconds of a duration.
pub fn seconds(duration: &Duration) -> i64 {
    duration.num_seconds()
}

/// One task, as every listing of tasks prints it.
#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: Option<i32>,
    /// The first task of the chain this one continues across days (the table's
    /// `TASK ID`); the task's own id when it starts one.
    pub chain_id: Option<i32>,
    /// When the row was written, local time.
    pub created_at: Option<String>,
    pub name: String,
    pub comment: String,
    /// Progress from 0 to 100; a task with none recorded counts as done, as the table shows it.
    pub completeness: i32,
    pub tags: Vec<String>,
    /// The Jira issue the task was taken from.
    pub jira_key: Option<String>,
}

impl From<&crate::libs::task::Task> for Task {
    fn from(task: &crate::libs::task::Task) -> Self {
        Task {
            id: task.id,
            chain_id: task.task_id,
            // Stored as `YYYY-MM-DD HH:MM:SS`; the contract spells moments with a `T`.
            created_at: task.timestamp.as_ref().map(|at| at.replacen(' ', "T", 1)),
            name: task.name.clone(),
            comment: task.comment.clone(),
            completeness: task.completeness.unwrap_or(100),
            tags: task.tags.iter().map(|tag| tag.name.clone()).collect(),
            jira_key: task.jira_key.clone(),
        }
    }
}

/// A list of tasks: `kasl task list`, `task show`.
#[derive(Debug, Serialize)]
pub struct Tasks {
    pub tasks: Vec<Task>,
}

impl Tasks {
    pub fn of(tasks: &[crate::libs::task::Task]) -> Self {
        Tasks {
            tasks: tasks.iter().map(Task::from).collect(),
        }
    }
}

/// One stretch of work between pauses.
#[derive(Debug, Serialize)]
pub struct Interval {
    #[serde(serialize_with = "moment::local")]
    pub start: NaiveDateTime,
    #[serde(serialize_with = "moment::local")]
    pub end: NaiveDateTime,
    pub seconds: i64,
}

impl From<&crate::libs::report::WorkInterval> for Interval {
    fn from(interval: &crate::libs::report::WorkInterval) -> Self {
        Interval {
            start: interval.start,
            end: interval.end,
            seconds: seconds(&interval.duration),
        }
    }
}

/// When a workday began and, once it has, ended.
#[derive(Debug, Serialize)]
pub struct Workday {
    #[serde(serialize_with = "moment::local")]
    pub start: NaiveDateTime,
    #[serde(serialize_with = "moment::local_opt")]
    pub end: Option<NaiveDateTime>,
}

/// Short intervals left out of a report, and how much time they held.
#[derive(Debug, Default, Serialize)]
pub struct Dropped {
    pub count: usize,
    pub seconds: i64,
}

/// The inbox at a glance: what is waiting, what is new, what is in hand.
#[derive(Debug, Default, Serialize)]
pub struct InboxCounts {
    pub total: i64,
    pub fresh: i64,
    pub taken: i64,
}

impl From<crate::db::jira_inbox::InboxCounts> for InboxCounts {
    fn from(counts: crate::db::jira_inbox::InboxCounts) -> Self {
        InboxCounts {
            total: counts.total,
            fresh: counts.fresh,
            taken: counts.taken,
        }
    }
}

/// One day: `kasl report`.
#[derive(Debug, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    /// `null` for a day nothing was recorded on.
    pub workday: Option<Workday>,
    /// The intervals the report counts, short ones already left out.
    pub intervals: Vec<Interval>,
    /// The sum of `intervals`.
    pub work_seconds: i64,
    pub productivity: Option<f64>,
    /// Whether productivity is below the configured minimum - the gate `report --send` enforces.
    pub below_threshold: bool,
    /// The short intervals `intervals` leaves out.
    pub dropped: Dropped,
    pub tasks: Vec<Task>,
    pub inbox: InboxCounts,
}

/// One day of a month's summary.
#[derive(Debug, Serialize)]
pub struct MonthDay {
    pub date: NaiveDate,
    pub work_seconds: i64,
    pub productivity: f64,
    /// A company rest day counted as a full day, not a recorded one.
    pub rest_day: bool,
}

/// A month: `kasl sum`.
#[derive(Debug, Serialize)]
pub struct Month {
    /// `YYYY-MM`.
    pub month: String,
    pub days: Vec<MonthDay>,
    pub total_seconds: i64,
    pub average_seconds: i64,
    /// The average over recorded days; `null` while there are none.
    pub productivity: Option<f64>,
}

/// One pause.
#[derive(Debug, Serialize)]
pub struct Pause {
    pub id: i32,
    #[serde(serialize_with = "moment::local")]
    pub start: NaiveDateTime,
    /// `null` while the pause is still running.
    #[serde(serialize_with = "moment::local_opt")]
    pub end: Option<NaiveDateTime>,
    pub seconds: Option<i64>,
    /// Recorded by hand (`pauses add`), and exempt from filtering and merging.
    pub kept: bool,
}

impl From<&crate::libs::pause::Pause> for Pause {
    fn from(pause: &crate::libs::pause::Pause) -> Self {
        Pause {
            id: pause.id,
            start: pause.start,
            end: pause.end,
            seconds: pause.duration.as_ref().map(seconds),
            kept: pause.protected,
        }
    }
}

/// A day's pauses: `kasl pauses list`.
#[derive(Debug, Serialize)]
pub struct Pauses {
    pub date: NaiveDate,
    /// The shortest pause listed; shorter ones are left out unless kept.
    pub min_minutes: u64,
    pub pauses: Vec<Pause>,
    pub total_seconds: i64,
}

/// One tag.
#[derive(Debug, Serialize)]
pub struct Tag {
    pub id: Option<i32>,
    pub name: String,
    pub color: Option<String>,
}

impl From<&crate::db::tags::Tag> for Tag {
    fn from(tag: &crate::db::tags::Tag) -> Self {
        Tag {
            id: tag.id,
            name: tag.name.clone(),
            color: tag.color.clone(),
        }
    }
}

/// Every tag: `kasl tag list`.
#[derive(Debug, Serialize)]
pub struct Tags {
    pub tags: Vec<Tag>,
}

/// A tag and the tasks carrying it: `kasl tag show`.
#[derive(Debug, Serialize)]
pub struct TagTasks {
    pub tag: Tag,
    pub tasks: Vec<Task>,
}

/// One template.
#[derive(Debug, Serialize)]
pub struct Template {
    pub name: String,
    pub task_name: String,
    pub comment: String,
    pub completeness: i32,
}

impl From<&crate::db::templates::TaskTemplate> for Template {
    fn from(template: &crate::db::templates::TaskTemplate) -> Self {
        Template {
            name: template.name.clone(),
            task_name: template.task_name.clone(),
            comment: template.comment.clone(),
            completeness: template.completeness,
        }
    }
}

/// One template: `kasl template show`.
#[derive(Debug, Serialize)]
pub struct TemplateShown {
    pub template: Template,
}

/// A list of templates: `kasl template list`, `search`.
#[derive(Debug, Serialize)]
pub struct Templates {
    pub templates: Vec<Template>,
}

impl Templates {
    pub fn of(templates: &[crate::db::templates::TaskTemplate]) -> Self {
        Templates {
            templates: templates.iter().map(Template::from).collect(),
        }
    }
}

/// One inbox issue.
///
/// The fields rigger reads (`key`, `summary`, `status`, `priority`, `score`,
/// `url`, `first_seen`, `taken_at`, `pinned`, `snoozed_until`, `gone_at`) are
/// the floor; the rest is what `kasl inbox show` prints.
#[derive(Debug, Serialize)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    /// The status's name; `null` until kasl has learnt the name for its id.
    pub status: Option<String>,
    pub status_id: Option<String>,
    pub priority: Option<String>,
    /// The ranking field's value (e.g. Scoring), when one is configured.
    pub score: Option<f64>,
    #[serde(rename = "type")]
    pub issue_type: Option<String>,
    pub project: Option<String>,
    pub url: String,
    pub pinned: bool,
    pub dismissed: bool,
    /// The rule behind the pin or the dismissal; `null` when a hand set it, or nothing did.
    pub ruled_by: Option<String>,
    /// The freshness mark the list shows: `NEW`, `gone`, `status→In Progress`.
    pub badge: Option<String>,
    #[serde(serialize_with = "moment::local")]
    pub first_seen: NaiveDateTime,
    #[serde(serialize_with = "moment::local")]
    pub last_seen: NaiveDateTime,
    #[serde(serialize_with = "moment::local_opt")]
    pub changed_at: Option<NaiveDateTime>,
    #[serde(serialize_with = "moment::local_opt")]
    pub taken_at: Option<NaiveDateTime>,
    #[serde(serialize_with = "moment::local_opt")]
    pub snoozed_until: Option<NaiveDateTime>,
    #[serde(serialize_with = "moment::local_opt")]
    pub gone_at: Option<NaiveDateTime>,
    /// Why a gone issue left: `resolved` or `moved`; `null` while present or not yet known.
    pub left: Option<&'static str>,
    /// The resolution Jira gave, when the issue left resolved.
    pub resolution: Option<String>,
}

impl Issue {
    /// The issue as it reads at `now` (the badge depends on it).
    pub fn at(item: &JiraInboxItem, now: NaiveDateTime) -> Self {
        Issue {
            key: item.issue_key.clone(),
            summary: item.summary.clone(),
            status: Some(item.status_name.clone()).filter(|name| !name.is_empty()),
            status_id: item.status_id.clone(),
            priority: item.priority.clone(),
            score: item.sort_value,
            issue_type: item.issue_type.clone(),
            project: item.project.clone(),
            url: item.url.clone(),
            pinned: item.pinned,
            dismissed: item.dismissed,
            ruled_by: item.ruled_by.clone(),
            badge: item.badge(now),
            first_seen: item.first_seen,
            last_seen: item.last_seen,
            changed_at: item.changed_at,
            taken_at: item.taken_at,
            snoozed_until: item.snoozed_until,
            gone_at: item.gone_at,
            left: item.left_reason.map(|reason| match reason {
                LeftReason::Resolved => "resolved",
                LeftReason::Moved => "moved",
            }),
            resolution: item.resolution.clone(),
        }
    }

    /// The issue as it reads now.
    pub fn now(item: &JiraInboxItem) -> Self {
        Self::at(item, Local::now().naive_local())
    }
}

/// The inbox, cut and ordered as asked: `kasl inbox list`.
#[derive(Debug, Serialize)]
pub struct InboxList {
    /// In the order the list shows them.
    pub issues: Vec<Issue>,
    /// How many issues the inbox holds before the filter and the limit; a
    /// shorter `issues` is a slice of this many.
    pub total: usize,
    /// The filter in words, as the list's header says it; `[]` for the whole inbox.
    pub filter: Vec<String>,
}

/// One line of where an issue's place comes from.
#[derive(Debug, Serialize)]
pub struct Reason {
    pub what: String,
    pub value: String,
    pub because: String,
}

impl From<&crate::libs::inbox_filter::Reason> for Reason {
    fn from(reason: &crate::libs::inbox_filter::Reason) -> Self {
        Reason {
            what: reason.what.to_string(),
            value: reason.value.clone(),
            because: reason.because.clone(),
        }
    }
}

/// One issue in full: `kasl inbox show`.
#[derive(Debug, Serialize)]
pub struct IssueShown {
    pub issue: Issue,
    /// Where its place in the list comes from, with `--why`; `null` without it.
    pub why: Option<Vec<Reason>>,
}

/// One inbox rule and where it stands.
#[derive(Debug, Serialize)]
pub struct Rule {
    /// Position in `jira_inbox.rules`, counting from 1.
    pub number: usize,
    pub name: Option<String>,
    /// The conditions, in the list's filter words.
    pub when: String,
    /// The actions.
    pub then: String,
    /// What it matches in the inbox now, in words.
    pub matches: String,
    /// Matching issues its pin or dismissal has not reached - what `--apply` would change.
    pub waiting: usize,
}

impl From<&crate::libs::inbox_rules::RuleStanding> for Rule {
    fn from(standing: &crate::libs::inbox_rules::RuleStanding) -> Self {
        Rule {
            number: standing.number,
            name: standing.name.clone(),
            when: standing.when.clone(),
            then: standing.then.clone(),
            matches: standing.now.clone(),
            waiting: standing.waiting,
        }
    }
}

/// A rule kasl could not read, and why.
#[derive(Debug, Serialize)]
pub struct RuleSkipped {
    pub number: usize,
    pub reason: String,
}

/// What the rules did to one issue.
#[derive(Debug, Serialize)]
pub struct RuleActed {
    pub key: String,
    /// The rule that pinned it.
    pub pinned: Option<String>,
    /// The rule that dismissed it.
    pub dismissed: Option<String>,
    /// The rule that asks for an urgent toast about it.
    pub urgent: Option<String>,
    /// Whether an urgent rule woke it from a snooze.
    pub woke: bool,
}

impl From<&crate::libs::inbox_rules::Ruled> for RuleActed {
    fn from(ruled: &crate::libs::inbox_rules::Ruled) -> Self {
        RuleActed {
            key: ruled.issue_key.clone(),
            pinned: ruled.pinned.clone(),
            dismissed: ruled.dismissed.clone(),
            urgent: ruled.urgent.clone(),
            woke: ruled.woke,
        }
    }
}

/// The inbox rules: `kasl inbox rules`.
#[derive(Debug, Serialize)]
pub struct Rules {
    pub rules: Vec<Rule>,
    pub skipped: Vec<RuleSkipped>,
    /// What `--apply` changed; `null` for a dry run.
    pub applied: Option<Vec<RuleActed>>,
}

/// One day the server is still owed.
#[derive(Debug, Serialize)]
pub struct OwedDay {
    pub date: NaiveDate,
    #[serde(serialize_with = "moment::local")]
    pub queued_at: NaiveDateTime,
    pub attempts: i32,
    #[serde(serialize_with = "moment::local_opt")]
    pub last_attempt_at: Option<NaiveDateTime>,
    pub last_error: Option<String>,
}

impl From<&crate::db::server_outbox::OwedDay> for OwedDay {
    fn from(day: &crate::db::server_outbox::OwedDay) -> Self {
        OwedDay {
            date: day.date,
            queued_at: day.queued_at,
            attempts: day.attempts,
            last_attempt_at: day.last_attempt_at,
            last_error: day.last_error.clone(),
        }
    }
}

/// The days waiting to reach the server: `kasl server queue`.
#[derive(Debug, Serialize)]
pub struct Queue {
    pub days: Vec<OwedDay>,
}

/// How the pulse is going, from what the watcher last recorded.
#[derive(Debug, Serialize)]
pub struct Pulse {
    pub enabled: bool,
    /// When a pulse last arrived.
    #[serde(serialize_with = "moment::utc_opt")]
    pub sent_at: Option<chrono::DateTime<chrono::Utc>>,
    /// What that pulse said: `working`, `paused`, `idle`.
    pub state: Option<&'static str>,
    /// When the watcher last tried, whatever came of it.
    #[serde(serialize_with = "moment::utc_opt")]
    pub attempted_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Why the last try did not arrive; `null` when it did.
    pub error: Option<String>,
    /// What `server status` warns about, in its words: failing, silent, shown offline, clock skew.
    pub warnings: Vec<String>,
}

/// The connection to a kasl-server: `kasl server status`.
///
/// Fields the checks did not reach are `null`: no token means the server was
/// never asked, an unreachable server means nobody was identified.
#[derive(Debug, Serialize)]
pub struct ServerStatus {
    pub connected: bool,
    pub url: Option<String>,
    /// Whether the agent token is in the OS keyring.
    pub token: Option<bool>,
    /// Whether the server answered its health check.
    pub reachable: Option<bool>,
    pub server_version: Option<String>,
    pub api_version: Option<String>,
    /// Whose token it is, by the server's account.
    pub user: Option<String>,
    pub agent: Option<String>,
    /// The first check that failed, in its words.
    pub problem: Option<String>,
    /// `null` when not connected.
    pub pulse: Option<Pulse>,
}

/// One kind of data the server stores.
#[derive(Debug, Serialize)]
pub struct Stored {
    pub what: String,
    pub detail: String,
}

/// What the connected server stores about this person: `kasl server manifest`.
#[derive(Debug, Serialize)]
pub struct Manifest {
    pub level: String,
    pub summary: String,
    pub stored: Vec<Stored>,
    pub never_collected: Vec<String>,
    pub visible_to: Vec<String>,
    pub retention: String,
    pub on_change: String,
    #[serde(serialize_with = "moment::utc_opt")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<crate::api::kasl_server::PrivacyManifest> for Manifest {
    fn from(manifest: crate::api::kasl_server::PrivacyManifest) -> Self {
        Manifest {
            level: manifest.level,
            summary: manifest.summary,
            stored: manifest
                .stored
                .into_iter()
                .map(|stored| Stored {
                    what: stored.what,
                    detail: stored.detail,
                })
                .collect(),
            never_collected: manifest.never_collected,
            visible_to: manifest.visible_to,
            retention: manifest.retention,
            on_change: manifest.on_change,
            updated_at: manifest.updated_at,
        }
    }
}

/// Whether kasl starts with the system: `kasl autostart status`.
#[derive(Debug, Serialize)]
pub struct Autostart {
    pub enabled: bool,
}

/// One field of the daily report's form.
#[derive(Debug, Serialize)]
pub struct Field {
    pub name: String,
    pub value: String,
}

/// What `report --send` would post: `kasl report --send --show`.
#[derive(Debug, Serialize)]
pub struct Payload {
    pub url: String,
    pub date: NaiveDate,
    /// Every field of the form, in order, as sent.
    pub fields: Vec<Field>,
    /// The `tasks` field's JSON, parsed.
    pub tasks: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moments_are_whole_local_seconds_with_a_t() {
        let at = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap().and_hms_nano_opt(9, 1, 2, 345_678_901).unwrap();
        let json = serde_json::to_value(Workday { start: at, end: None }).unwrap();
        assert_eq!(json, serde_json::json!({ "start": "2026-10-15T09:01:02", "end": null }));
    }

    #[test]
    fn a_task_timestamp_takes_the_t_and_a_task_without_completeness_reads_done() {
        let mut task = crate::libs::task::Task::new("Ship", "", None);
        task.timestamp = Some("2026-10-15 09:01:02".to_string());
        task.completeness = None;
        let json = Task::from(&task);
        assert_eq!(json.created_at.as_deref(), Some("2026-10-15T09:01:02"));
        assert_eq!(json.completeness, 100);
    }

    #[test]
    fn percentages_keep_one_decimal() {
        assert_eq!(percent(87.345), 87.3);
        assert_eq!(percent(100.0), 100.0);
    }
}
