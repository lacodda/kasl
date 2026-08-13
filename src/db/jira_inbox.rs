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
            return Some("gone".to_string());
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
}

/// An existing issue whose tracked fields changed during a sync.
#[derive(Debug, Clone)]
pub struct ChangedIssue {
    pub issue_key: String,
    /// Human-readable change summary, e.g. `status→In Progress, score 5→8`.
    pub change: String,
    pub dismissed: bool,
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
                        gone_at = NULL
                     WHERE issue_key = ?10",
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
                        item.issue_key,
                    ],
                )?;
                result.updated += 1;

                let change = describe_change(&old, item);
                if let Some(change) = change {
                    self.db.conn.execute(
                        "UPDATE jira_inbox SET last_change = ?1, changed_at = ?2 WHERE issue_key = ?3",
                        params![change, now, item.issue_key],
                    )?;
                    result.changed.push(ChangedIssue {
                        issue_key: item.issue_key.clone(),
                        change,
                        dismissed: old.dismissed,
                    });
                }
            } else {
                self.db.conn.execute(
                    "INSERT INTO jira_inbox (
                        issue_key, issue_id, summary, status_id, priority, priority_rank,
                        sort_value, url, first_seen, last_seen, notified, pinned, dismissed, raw_updated
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, 0, 0, ?11)",
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

    /// Active (non-dismissed) items: pinned, then sort_value DESC, then priority.
    ///
    /// Gone issues are hidden unless `include_gone` is set (`--all`), in which
    /// case they sort below the present ones.
    pub fn list_active(&self, include_gone: bool) -> Result<Vec<JiraInboxItem>> {
        let gone_filter = if include_gone { "" } else { " AND i.gone_at IS NULL" };
        let query = format!(
            "SELECT i.issue_key, i.issue_id, i.summary, i.status_id, COALESCE(s.name, ''),
                    i.priority, i.priority_rank, i.sort_value, i.url,
                    i.first_seen, i.last_seen, i.notified, i.pinned, i.dismissed, i.raw_updated,
                    i.gone_at, i.last_change, i.changed_at
             FROM jira_inbox i
             LEFT JOIN jira_statuses s ON s.id = i.status_id
             WHERE i.dismissed = 0{gone_filter}
             ORDER BY i.gone_at IS NOT NULL, i.pinned DESC, i.sort_value IS NULL, i.sort_value DESC,
                      i.priority_rank ASC, i.last_seen DESC"
        );
        let mut stmt = self.db.conn.prepare(&query)?;

        let rows = stmt.query_map([], map_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_by_key(&self, key: &str) -> Result<Option<JiraInboxItem>> {
        self.db
            .conn
            .query_row(
                "SELECT i.issue_key, i.issue_id, i.summary, i.status_id, COALESCE(s.name, ''),
                        i.priority, i.priority_rank, i.sort_value, i.url,
                        i.first_seen, i.last_seen, i.notified, i.pinned, i.dismissed, i.raw_updated,
                        i.gone_at, i.last_change, i.changed_at
                 FROM jira_inbox i
                 LEFT JOIN jira_statuses s ON s.id = i.status_id
                 WHERE i.issue_key = ?1",
                params![key],
                map_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn set_pinned(&self, key: &str, pinned: bool) -> Result<bool> {
        let n = self
            .db
            .conn
            .execute("UPDATE jira_inbox SET pinned = ?1 WHERE issue_key = ?2", params![pinned as i32, key])?;
        Ok(n > 0)
    }

    pub fn set_dismissed(&self, key: &str, dismissed: bool) -> Result<bool> {
        let n = self
            .db
            .conn
            .execute("UPDATE jira_inbox SET dismissed = ?1 WHERE issue_key = ?2", params![dismissed as i32, key])?;
        Ok(n > 0)
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

/// Builds a human-readable change summary, or `None` when nothing visible changed.
fn describe_change(old: &ExistingRow, new: &JiraInboxUpsert) -> Option<String> {
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

    let score_changed = match (old.sort_value, new.sort_value) {
        (Some(a), Some(b)) => (a - b).abs() > f64::EPSILON,
        (None, None) => false,
        _ => true,
    };
    if score_changed {
        let fmt = |v: Option<f64>| v.map(fmt_score).unwrap_or_else(|| "—".to_string());
        parts.push(format!("score {}→{}", fmt(old.sort_value), fmt(new.sort_value)));
    }

    if parts.is_empty() { None } else { Some(parts.join(", ")) }
}

/// Formats a score without a trailing `.0` for whole numbers.
fn fmt_score(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

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
    })
}
