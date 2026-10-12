//! Persistent store for discovered Jira inbox issues.
//!
//! Holds assigned issues synced from Jira so the watcher can toast on
//! new arrivals and visible changes, and the CLI can list, pin, dismiss,
//! open, and import them. Issues that stop appearing in the poll are
//! reconciled with `gone_at` instead of lingering forever.

use crate::db::db::Db;
use anyhow::Result;
use chrono::{Duration, Local, NaiveDateTime};
use rusqlite::{OptionalExtension, params};

/// How long a NEW / change badge stays visible in the list.
pub const FRESH_BADGE_HOURS: i64 = 24;

/// A single Jira issue tracked in the local inbox.
#[derive(Debug, Clone)]
pub struct JiraInboxItem {
    pub issue_key: String,
    pub issue_id: String,
    pub summary: String,
    pub status_id: Option<String>,
    /// Resolved status name (from `jira_statuses` join), may be empty.
    pub status_name: String,
    pub priority: Option<String>,
    pub priority_rank: i32,
    /// Numeric ranking value from configured sort custom field (e.g. Scoring).
    pub sort_value: Option<f64>,
    pub url: String,
    pub first_seen: NaiveDateTime,
    pub last_seen: NaiveDateTime,
    pub notified: bool,
    pub pinned: bool,
    pub dismissed: bool,
    pub raw_updated: Option<String>,
    /// When the issue stopped appearing in the Jira poll (closed, reassigned).
    pub gone_at: Option<NaiveDateTime>,
    /// Most recent visible change, e.g. `status→In Progress` or `↑prio High`.
    pub last_change: Option<String>,
    pub changed_at: Option<NaiveDateTime>,
    /// When `inbox take` turned this issue into a task.
    ///
    /// A taken issue stays in the list - the point is to see what is in hand,
    /// not to hide it. Dismissal remains separate and still means "not mine".
    pub taken_at: Option<NaiveDateTime>,
    /// When a snoozed issue is due back, if it is asleep.
    ///
    /// Dismissal says "never"; this says "not now". A sleeping issue is out
    /// of the list until the moment passes, then it is in it again - which is
    /// the whole point, and why the date is stored rather than a flag.
    pub snoozed_until: Option<NaiveDateTime>,
    /// When a snooze last ran out, so the return is announced once.
    pub woke_at: Option<NaiveDateTime>,
    /// Why a taken issue left the inbox, once Jira has said.
    ///
    /// `None` while the issue is present, for issues never taken (kasl does
    /// not ask about those), and for a departure not yet explained.
    pub left_reason: Option<LeftReason>,
    /// The resolution Jira gave, when the issue left resolved ("Done", "Won't Fix").
    pub resolution: Option<String>,
    /// The issue type in the source's own words (Bug, Story, «Ошибка»).
    ///
    /// Empty for a row the poll has not refreshed since the upgrade that
    /// added it.
    pub issue_type: Option<String>,
    /// The project the issue belongs to: in Jira, the prefix of its key.
    pub project: Option<String>,
    /// The rule behind the row's pin or dismissal; `None` when a hand set it, or nothing did.
    pub ruled_by: Option<String>,
}

/// Which rows a listing includes besides the ones asking for attention now.
///
/// Every flag widens; none narrows. The default is the inbox as `kasl inbox`
/// shows it: present, awake, not dismissed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Include {
    /// Issues that left the poll (`--all`).
    pub gone: bool,
    /// Issues still asleep (`--snoozed`).
    pub snoozed: bool,
    /// Issues dismissed by hand or by a rule (`--dismissed`).
    pub dismissed: bool,
}

impl Include {
    /// Every row the table holds.
    pub const EVERYTHING: Include = Include {
        gone: true,
        snoozed: true,
        dismissed: true,
    };
}

/// What a rule does to a row on its own authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuledMark {
    Pin,
    Dismiss,
}

/// Why an issue left the inbox.
///
/// The poll sees only absence; these are the two things absence can mean, and
/// they ask for opposite things - a resolved issue's task is ready to be marked
/// done, a moved issue's is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftReason {
    /// Jira resolved it.
    Resolved,
    /// Unresolved but no longer yours: reassigned, moved out of reach, deleted.
    Moved,
}

impl LeftReason {
    /// The stored spelling; the column's CHECK accepts exactly these.
    pub fn as_str(self) -> &'static str {
        match self {
            LeftReason::Resolved => "resolved",
            LeftReason::Moved => "moved",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "resolved" => Some(LeftReason::Resolved),
            "moved" => Some(LeftReason::Moved),
            _ => None,
        }
    }
}

/// What the inbox holds right now, for the daily report line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InboxCounts {
    /// Issues still asking for attention (not dismissed, not gone).
    pub total: i64,
    /// Of those, already turned into tasks.
    pub taken: i64,
    /// Of those, first seen within the freshness window and not yet taken.
    pub fresh: i64,
}

impl InboxCounts {
    /// Nothing to say when the inbox is empty.
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }
}

impl JiraInboxItem {
    /// Badge for the list view: `gone`, `NEW`, or the recent change text.
    ///
    /// `gone` always wins (such rows only show up with `--all`); a freshly
    /// discovered issue shows `NEW`; otherwise a change within the freshness
    /// window shows its description. Older rows get no badge.
    pub fn badge(&self, now: NaiveDateTime) -> Option<String> {
        let fresh = |t: NaiveDateTime| now.signed_duration_since(t) < Duration::hours(FRESH_BADGE_HOURS);
        if self.gone_at.is_some() {
            // Said only when Jira has said it: kasl asks why a taken issue
            // left, and "gone" is the honest word for everything else.
            if self.left_reason == Some(LeftReason::Resolved) {
                return Some("resolved".to_string());
            }
            return Some("gone".to_string());
        }
        // Only `--dismissed` shows these, and who hid it is what is worth
        // seeing: a rule that hides the wrong issues is found here.
        if self.dismissed {
            return Some(match &self.ruled_by {
                Some(rule) => format!("ruled out: {rule}"),
                None => "dismissed".to_string(),
            });
        }
        // Only `--snoozed` shows a sleeping issue at all, and when it does,
        // when it is due is the only thing worth saying about it.
        if let Some(until) = self.snoozed_until
            && until > now
        {
            return Some(format!("zzz {}", until.format("%b %-d")));
        }
        // Taken outranks NEW and change badges: once an issue is in hand, that
        // is the fact worth seeing, and it does not fade with time.
        if self.taken_at.is_some() {
            return Some("taken".to_string());
        }
        // A snooze that just ran out is why the issue is back in the list;
        // without saying so it reads as an issue that was never away.
        if let Some(woke) = self.woke_at
            && fresh(woke)
        {
            return Some("back".to_string());
        }
        if fresh(self.first_seen) {
            return Some("NEW".to_string());
        }
        match (&self.last_change, self.changed_at) {
            (Some(change), Some(at)) if fresh(at) => Some(change.clone()),
            _ => None,
        }
    }
}

/// Input row used when upserting issues from a Jira poll.
#[derive(Debug, Clone)]
pub struct JiraInboxUpsert {
    pub issue_key: String,
    pub issue_id: String,
    pub summary: String,
    pub status_id: Option<String>,
    /// Status display name for change descriptions (not stored).
    pub status_name: String,
    pub priority: Option<String>,
    pub priority_rank: i32,
    pub sort_value: Option<f64>,
    pub url: String,
    pub raw_updated: Option<String>,
    pub issue_type: Option<String>,
    pub project: Option<String>,
}

/// An existing issue whose tracked fields changed during a sync.
#[derive(Debug, Clone)]
pub struct ChangedIssue {
    pub issue_key: String,
    /// Human-readable change summary, e.g. `status→In Progress, score 5→8`.
    pub change: String,
    pub dismissed: bool,
    /// Whether a person moved the issue: its status or priority changed.
    ///
    /// Only these are worth interrupting for. A score is derived - Jira-side
    /// automation recomputes it for every issue, often in batches over the
    /// day - and "back" is the inbox correcting its own view after a poll that
    /// missed the issue. Both stay on the badge; neither raises a toast,
    /// because both arrive for the whole pile at once and say nothing new
    /// about any one issue.
    pub notable: bool,
}

/// Result of an upsert batch: new keys and visibly changed issues.
#[derive(Debug, Default)]
pub struct UpsertBatchResult {
    pub new_keys: Vec<String>,
    pub updated: usize,
    pub changed: Vec<ChangedIssue>,
}

/// Database operations for the Jira inbox table.
pub struct JiraInbox {
    db: Db,
}

impl JiraInbox {
    pub fn new() -> Result<Self> {
        Ok(Self { db: Db::new()? })
    }

    /// Inserts new issues and refreshes metadata for existing ones.
    ///
    /// Detects visible changes (status, priority, score) on existing rows and
    /// clears `gone_at` for issues that reappeared in the poll. Returns brand
    /// new keys (toast candidates) and the changed issues with descriptions.
    pub fn upsert_batch(&self, items: &[JiraInboxUpsert]) -> Result<UpsertBatchResult> {
        let now = Local::now().naive_local();
        let mut result = UpsertBatchResult::default();

        for item in items {
            if let Some(name) = &item.priority {
                self.db.conn.execute(
                    "INSERT INTO jira_priorities (name, rank) VALUES (?1, ?2)
                     ON CONFLICT(name) DO UPDATE SET rank = excluded.rank",
                    params![name, item.priority_rank],
                )?;
            }
            let existing: Option<ExistingRow> = self
                .db
                .conn
                .query_row(
                    "SELECT status_id, priority, priority_rank, sort_value, gone_at, dismissed
                     FROM jira_inbox WHERE issue_key = ?1",
                    params![item.issue_key],
                    |row| {
                        Ok(ExistingRow {
                            status_id: row.get(0)?,
                            priority: row.get(1)?,
                            priority_rank: row.get(2)?,
                            sort_value: row.get(3)?,
                            gone_at: row.get(4)?,
                            dismissed: row.get::<_, i32>(5)? != 0,
                        })
                    },
                )
                .optional()?;

            if let Some(old) = existing {
                self.db.conn.execute(
                    "UPDATE jira_inbox SET
                        issue_id = ?1,
                        summary = ?2,
                        status_id = ?3,
                        priority = ?4,
                        priority_rank = ?5,
                        sort_value = ?6,
                        url = ?7,
                        last_seen = ?8,
                        raw_updated = ?9,
                        issue_type = ?10,
                        project = ?11,
                        gone_at = NULL,
                        left_reason = NULL,
                        resolution = NULL
                     WHERE issue_key = ?12",
                    params![
                        item.issue_id,
                        item.summary,
                        item.status_id,
                        item.priority,
                        item.priority_rank,
                        item.sort_value,
                        item.url,
                        now,
                        item.raw_updated,
                        item.issue_type,
                        item.project,
                        item.issue_key,
                    ],
                )?;
                result.updated += 1;

                if let Some(Change { text: change, notable }) = describe_change(&old, item) {
                    self.db.conn.execute(
                        "UPDATE jira_inbox SET last_change = ?1, changed_at = ?2 WHERE issue_key = ?3",
                        params![change, now, item.issue_key],
                    )?;
                    result.changed.push(ChangedIssue {
                        issue_key: item.issue_key.clone(),
                        change,
                        dismissed: old.dismissed,
                        notable,
                    });
                }
            } else {
                self.db.conn.execute(
                    "INSERT INTO jira_inbox (
                        issue_key, issue_id, summary, status_id, priority, priority_rank,
                        sort_value, url, first_seen, last_seen, notified, pinned, dismissed, raw_updated,
                        issue_type, project
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, 0, 0, ?11, ?12, ?13)",
                    params![
                        item.issue_key,
                        item.issue_id,
                        item.summary,
                        item.status_id,
                        item.priority,
                        item.priority_rank,
                        item.sort_value,
                        item.url,
                        now,
                        now,
                        item.raw_updated,
                        item.issue_type,
                        item.project,
                    ],
                )?;
                result.new_keys.push(item.issue_key.clone());
            }
        }

        Ok(result)
    }

    /// Marks issues missing from the current poll as gone.
    ///
    /// Every non-gone row whose key is not in `present_keys` gets `gone_at`
    /// stamped. Returns keys that were both visible (not dismissed) and newly
    /// gone — the candidates for a "left the inbox" toast.
    pub fn mark_gone(&self, present_keys: &[String]) -> Result<Vec<String>> {
        let now = Local::now().naive_local();
        let placeholders = vec!["?"; present_keys.len()].join(", ");
        let not_in = if present_keys.is_empty() {
            String::new()
        } else {
            format!(" AND issue_key NOT IN ({placeholders})")
        };

        let select = format!("SELECT issue_key FROM jira_inbox WHERE gone_at IS NULL AND dismissed = 0{not_in}");
        let mut stmt = self.db.conn.prepare(&select)?;
        let newly_gone: Vec<String> = stmt
            .query_map(rusqlite::params_from_iter(present_keys.iter()), |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let update = format!("UPDATE jira_inbox SET gone_at = ?1 WHERE gone_at IS NULL{not_in}");
        let mut update_params: Vec<&dyn rusqlite::types::ToSql> = vec![&now];
        for key in present_keys {
            update_params.push(key);
        }
        self.db.conn.execute(&update, &update_params[..])?;

        Ok(newly_gone)
    }

    /// Taken issues that left the inbox and whose reason kasl has not learnt yet.
    ///
    /// Oldest departure first, at most `limit`: the first poll after an
    /// upgrade finds every taken issue that ever left unexplained, and asking
    /// Jira about all of them in one poll would stall it. The rest are asked
    /// about on the polls that follow. Dismissed rows are included - a
    /// dismissed issue that was taken still has a task waiting on the answer.
    pub fn unexplained_departures(&self, limit: usize) -> Result<Vec<String>> {
        let mut stmt = self.db.conn.prepare(
            "SELECT issue_key FROM jira_inbox
             WHERE gone_at IS NOT NULL AND taken_at IS NOT NULL AND left_reason IS NULL
             ORDER BY gone_at, issue_key
             LIMIT ?1",
        )?;
        let keys = stmt.query_map(params![limit as i64], |row| row.get(0))?.collect::<Result<Vec<String>, _>>()?;
        Ok(keys)
    }

    /// Sets the issue's status, as kasl itself just moved it in Jira.
    ///
    /// Leaves the change badge alone: the change is the person's own, and the
    /// badge is for news.
    pub fn set_status(&self, key: &str, status_id: &str) -> Result<bool> {
        let n = self
            .db
            .conn
            .execute("UPDATE jira_inbox SET status_id = ?1 WHERE issue_key = ?2", params![status_id, key])?;
        Ok(n > 0)
    }

    /// Records why an issue left: resolved with `resolution`, or moved away.
    pub fn set_left(&self, key: &str, reason: LeftReason, resolution: Option<&str>) -> Result<bool> {
        let n = self.db.conn.execute(
            "UPDATE jira_inbox SET left_reason = ?1, resolution = ?2 WHERE issue_key = ?3 AND gone_at IS NOT NULL",
            params![reason.as_str(), resolution, key],
        )?;
        Ok(n > 0)
    }

    /// Active (non-dismissed, awake) items: pinned, then sort_value DESC, then priority.
    ///
    /// Gone issues are hidden unless `include_gone` is set (`--all`), in which
    /// case they sort below the present ones. Sleeping issues are hidden the
    /// same way: a snooze that still showed the issue would be a no-op.
    pub fn list_active(&self, include_gone: bool) -> Result<Vec<JiraInboxItem>> {
        self.list(Include {
            gone: include_gone,
            ..Include::default()
        })
    }

    /// The inbox in its own order, widened by `include`.
    ///
    /// `--snoozed` is what makes a snooze trustworthy - there is a way to see
    /// what is in it - and `--dismissed` is the same for dismissal, which a
    /// rule can now do without anyone pressing a key.
    ///
    /// The clock is SQLite's own local time rather than a bound parameter, so
    /// "due" means the same thing here as in [`Self::wake_due`] - two readings
    /// of the same moment could otherwise disagree about one row.
    pub fn list(&self, include: Include) -> Result<Vec<JiraInboxItem>> {
        let mut filters = Vec::new();
        if !include.dismissed {
            filters.push("i.dismissed = 0");
        }
        if !include.gone {
            filters.push("i.gone_at IS NULL");
        }
        if !include.snoozed {
            filters.push("(i.snoozed_until IS NULL OR i.snoozed_until <= datetime(CURRENT_TIMESTAMP, 'localtime'))");
        }
        let filter = if filters.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", filters.join(" AND "))
        };
        let query = format!(
            "{SELECT_ITEM}{filter}
             ORDER BY i.gone_at IS NOT NULL, i.dismissed, i.pinned DESC, i.sort_value IS NULL, i.sort_value DESC,
                      i.priority_rank ASC, i.last_seen DESC"
        );
        let mut stmt = self.db.conn.prepare(&query)?;

        let rows = stmt.query_map([], map_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_by_key(&self, key: &str) -> Result<Option<JiraInboxItem>> {
        self.db
            .conn
            .query_row(&format!("{SELECT_ITEM} WHERE i.issue_key = ?1"), params![key], map_row)
            .optional()
            .map_err(Into::into)
    }

    /// Every priority the inbox has ever seen, with its rank.
    ///
    /// Priority names belong to the Jira instance, so "High+" - in a rule or
    /// on the command line - is read against these rather than a table of
    /// English defaults. A name learnt once stays known: the vocabulary must
    /// not shrink because the last High issue was lowered.
    pub fn priorities(&self) -> Result<Vec<(String, i32)>> {
        let mut stmt = self.db.conn.prepare("SELECT name, rank FROM jira_priorities ORDER BY rank, name")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Pins or unpins by hand, which takes the decision back from any rule.
    pub fn set_pinned(&self, key: &str, pinned: bool) -> Result<bool> {
        let n = self.db.conn.execute(
            "UPDATE jira_inbox SET pinned = ?1, ruled_by = NULL WHERE issue_key = ?2",
            params![pinned as i32, key],
        )?;
        Ok(n > 0)
    }

    /// Dismisses or restores by hand, which takes the decision back from any rule.
    pub fn set_dismissed(&self, key: &str, dismissed: bool) -> Result<bool> {
        let n = self.db.conn.execute(
            "UPDATE jira_inbox SET dismissed = ?1, ruled_by = NULL WHERE issue_key = ?2",
            params![dismissed as i32, key],
        )?;
        Ok(n > 0)
    }

    /// Pins or dismisses on a rule's authority, naming the rule.
    pub fn set_ruled(&self, key: &str, mark: RuledMark, rule: &str) -> Result<bool> {
        let column = match mark {
            RuledMark::Pin => "pinned",
            RuledMark::Dismiss => "dismissed",
        };
        let n = self.db.conn.execute(
            &format!("UPDATE jira_inbox SET {column} = 1, ruled_by = ?1 WHERE issue_key = ?2"),
            params![rule, key],
        )?;
        Ok(n > 0)
    }

    /// Counts what is waiting in the inbox, for the daily report line.
    ///
    /// Dismissed and gone rows are excluded - the point is what still asks for
    /// attention, which is the same set `inbox list` shows.
    pub fn counts(&self) -> Result<InboxCounts> {
        let now = Local::now().naive_local();
        let fresh_since = now - Duration::hours(FRESH_BADGE_HOURS);
        self.db
            .conn
            .query_row(
                "SELECT COUNT(*),
                        COUNT(taken_at),
                        SUM(CASE WHEN taken_at IS NULL AND first_seen >= ?1 THEN 1 ELSE 0 END)
                 FROM jira_inbox
                 WHERE dismissed = 0 AND gone_at IS NULL
                   AND (snoozed_until IS NULL OR snoozed_until <= ?2)",
                params![fresh_since, now],
                |row| {
                    Ok(InboxCounts {
                        total: row.get(0)?,
                        taken: row.get(1)?,
                        fresh: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    })
                },
            )
            .map_err(Into::into)
    }

    /// Marks the issue as taken, or clears the mark.
    ///
    /// Unlike dismissal this keeps the row in the list: `take` used to hide
    /// the issue, which lost the fact that it had been picked up at all.
    pub fn set_taken(&self, key: &str, taken: bool) -> Result<bool> {
        let value = taken.then(|| Local::now().naive_local());
        let n = self
            .db
            .conn
            .execute("UPDATE jira_inbox SET taken_at = ?1 WHERE issue_key = ?2", params![value, key])?;
        Ok(n > 0)
    }

    /// Puts an issue to sleep until `until`, or wakes it when `None`.
    ///
    /// Snoozing clears `woke_at`: the mark exists to announce one return, and
    /// a stale one would announce the wrong sleep.
    pub fn set_snoozed(&self, key: &str, until: Option<NaiveDateTime>) -> Result<bool> {
        let n = self.db.conn.execute(
            "UPDATE jira_inbox SET snoozed_until = ?1, woke_at = NULL WHERE issue_key = ?2",
            params![until, key],
        )?;
        Ok(n > 0)
    }

    /// Wakes every issue whose snooze has run out, and returns them.
    ///
    /// Waking is local bookkeeping, so it does not wait on a Jira poll: an
    /// issue deferred to Monday comes back on Monday whether or not the VPN
    /// is up. `snoozed_until` is cleared and `woke_at` stamped in the same
    /// statement, so an issue is returned here exactly once - a second caller,
    /// or a second poll a minute later, finds nothing left to wake.
    pub fn wake_due(&self) -> Result<Vec<JiraInboxItem>> {
        let now = Local::now().naive_local();
        let mut stmt = self
            .db
            .conn
            .prepare("SELECT issue_key FROM jira_inbox WHERE snoozed_until IS NOT NULL AND snoozed_until <= ?1 AND dismissed = 0")?;
        let due: Vec<String> = stmt.query_map(params![now], |row| row.get(0))?.collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut woken = Vec::with_capacity(due.len());
        for key in due {
            self.db.conn.execute(
                "UPDATE jira_inbox SET snoozed_until = NULL, woke_at = ?1 WHERE issue_key = ?2",
                params![now, key],
            )?;
            if let Some(item) = self.get_by_key(&key)? {
                woken.push(item);
            }
        }
        Ok(woken)
    }

    /// How many issues are asleep right now.
    pub fn count_snoozed(&self) -> Result<i64> {
        let now = Local::now().naive_local();
        self.db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM jira_inbox WHERE dismissed = 0 AND gone_at IS NULL AND snoozed_until > ?1",
                params![now],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn mark_notified(&self, keys: &[String]) -> Result<()> {
        for key in keys {
            self.db.conn.execute("UPDATE jira_inbox SET notified = 1 WHERE issue_key = ?1", params![key])?;
        }
        Ok(())
    }

    /// Un-notified newly inserted items (for toast after sync).
    pub fn list_unnotified_new(&self, keys: &[String]) -> Result<Vec<JiraInboxItem>> {
        let mut items = Vec::new();
        for key in keys {
            if let Some(item) = self.get_by_key(key)?
                && !item.notified
                && !item.dismissed
            {
                items.push(item);
            }
        }
        Ok(items)
    }
}

/// Tracked fields of an existing row, used for change detection.
struct ExistingRow {
    status_id: Option<String>,
    priority: Option<String>,
    priority_rank: i32,
    sort_value: Option<f64>,
    gone_at: Option<NaiveDateTime>,
    dismissed: bool,
}

/// A visible change on an existing row: what to show, and whether to toast it.
struct Change {
    text: String,
    notable: bool,
}

/// Builds a human-readable change summary, or `None` when nothing visible changed.
fn describe_change(old: &ExistingRow, new: &JiraInboxUpsert) -> Option<Change> {
    let mut parts = Vec::new();

    if old.gone_at.is_some() {
        parts.push("back".to_string());
    }

    if old.status_id != new.status_id {
        let name = if new.status_name.is_empty() {
            new.status_id.as_deref().unwrap_or("—")
        } else {
            &new.status_name
        };
        parts.push(format!("status→{name}"));
    }

    if old.priority_rank != new.priority_rank || old.priority != new.priority {
        let name = new.priority.as_deref().unwrap_or("—");
        // Lower rank = higher priority (rank sorts ascending).
        let arrow = if new.priority_rank < old.priority_rank {
            "↑prio"
        } else if new.priority_rank > old.priority_rank {
            "↓prio"
        } else {
            "prio→"
        };
        parts.push(format!("{arrow} {name}"));
    }

    let notable = old.status_id != new.status_id || old.priority_rank != new.priority_rank || old.priority != new.priority;

    let score_changed = match (old.sort_value, new.sort_value) {
        (Some(a), Some(b)) => (a - b).abs() > f64::EPSILON,
        (None, None) => false,
        _ => true,
    };
    if score_changed {
        let fmt = |v: Option<f64>| v.map(fmt_score).unwrap_or_else(|| "—".to_string());
        parts.push(format!("score {}→{}", fmt(old.sort_value), fmt(new.sort_value)));
    }

    if parts.is_empty() {
        None
    } else {
        Some(Change {
            text: parts.join(", "),
            notable,
        })
    }
}

/// Formats a score without a trailing `.0` for whole numbers.
fn fmt_score(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

/// The columns [`map_row`] reads, in its order, with the status name joined in.
const SELECT_ITEM: &str = "SELECT i.issue_key, i.issue_id, i.summary, i.status_id, COALESCE(s.name, ''),
        i.priority, i.priority_rank, i.sort_value, i.url,
        i.first_seen, i.last_seen, i.notified, i.pinned, i.dismissed, i.raw_updated,
        i.gone_at, i.last_change, i.changed_at, i.taken_at, i.snoozed_until, i.woke_at,
        i.left_reason, i.resolution, i.issue_type, i.project, i.ruled_by
     FROM jira_inbox i
     LEFT JOIN jira_statuses s ON s.id = i.status_id";

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JiraInboxItem> {
    Ok(JiraInboxItem {
        issue_key: row.get(0)?,
        issue_id: row.get(1)?,
        summary: row.get(2)?,
        status_id: row.get(3)?,
        status_name: row.get(4)?,
        priority: row.get(5)?,
        priority_rank: row.get(6)?,
        sort_value: row.get(7)?,
        url: row.get(8)?,
        first_seen: row.get(9)?,
        last_seen: row.get(10)?,
        notified: row.get::<_, i32>(11)? != 0,
        pinned: row.get::<_, i32>(12)? != 0,
        dismissed: row.get::<_, i32>(13)? != 0,
        raw_updated: row.get(14)?,
        gone_at: row.get(15)?,
        last_change: row.get(16)?,
        changed_at: row.get(17)?,
        taken_at: row.get(18)?,
        snoozed_until: row.get(19)?,
        woke_at: row.get(20)?,
        left_reason: row.get::<_, Option<String>>(21)?.as_deref().and_then(LeftReason::parse),
        resolution: row.get(22)?,
        issue_type: row.get(23)?,
        project: row.get(24)?,
        ruled_by: row.get(25)?,
    })
}
