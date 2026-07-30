//! Persistent store for discovered Jira inbox issues.
//!
//! Holds assigned open issues synced from Jira so the watcher can toast on
//! new arrivals and the CLI can list, pin, dismiss, open, and import them.

use crate::db::db::Db;
use anyhow::Result;
use chrono::{Local, NaiveDateTime};
use rusqlite::{params, OptionalExtension};

/// A single Jira issue tracked in the local inbox.
#[derive(Debug, Clone)]
pub struct JiraInboxItem {
    pub issue_key: String,
    pub issue_id: String,
    pub summary: String,
    pub status: String,
    pub priority: Option<String>,
    pub priority_rank: i32,
    pub url: String,
    pub first_seen: NaiveDateTime,
    pub last_seen: NaiveDateTime,
    pub notified: bool,
    pub pinned: bool,
    pub dismissed: bool,
    pub raw_updated: Option<String>,
}

/// Input row used when upserting issues from a Jira poll.
#[derive(Debug, Clone)]
pub struct JiraInboxUpsert {
    pub issue_key: String,
    pub issue_id: String,
    pub summary: String,
    pub status: String,
    pub priority: Option<String>,
    pub priority_rank: i32,
    pub url: String,
    pub raw_updated: Option<String>,
}

/// Result of an upsert batch: which keys were brand new.
#[derive(Debug, Default)]
pub struct UpsertBatchResult {
    pub new_keys: Vec<String>,
    pub updated: usize,
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
    /// Returns keys that did not previously exist (candidates for toast).
    pub fn upsert_batch(&self, items: &[JiraInboxUpsert]) -> Result<UpsertBatchResult> {
        let now = Local::now().naive_local();
        let mut result = UpsertBatchResult::default();

        for item in items {
            let existing: Option<String> = self
                .db
                .conn
                .query_row(
                    "SELECT issue_key FROM jira_inbox WHERE issue_key = ?1",
                    params![item.issue_key],
                    |row| row.get(0),
                )
                .optional()?;

            if existing.is_some() {
                self.db.conn.execute(
                    "UPDATE jira_inbox SET
                        issue_id = ?1,
                        summary = ?2,
                        status = ?3,
                        priority = ?4,
                        priority_rank = ?5,
                        url = ?6,
                        last_seen = ?7,
                        raw_updated = ?8
                     WHERE issue_key = ?9",
                    params![
                        item.issue_id,
                        item.summary,
                        item.status,
                        item.priority,
                        item.priority_rank,
                        item.url,
                        now,
                        item.raw_updated,
                        item.issue_key,
                    ],
                )?;
                result.updated += 1;
            } else {
                self.db.conn.execute(
                    "INSERT INTO jira_inbox (
                        issue_key, issue_id, summary, status, priority, priority_rank,
                        url, first_seen, last_seen, notified, pinned, dismissed, raw_updated
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 0, 0, ?10)",
                    params![
                        item.issue_key,
                        item.issue_id,
                        item.summary,
                        item.status,
                        item.priority,
                        item.priority_rank,
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

    /// Active (non-dismissed) items, pinned first then by priority and recency.
    pub fn list_active(&self) -> Result<Vec<JiraInboxItem>> {
        let mut stmt = self.db.conn.prepare(
            "SELECT issue_key, issue_id, summary, status, priority, priority_rank, url,
                    first_seen, last_seen, notified, pinned, dismissed, raw_updated
             FROM jira_inbox
             WHERE dismissed = 0
             ORDER BY pinned DESC, priority_rank ASC, last_seen DESC",
        )?;

        let rows = stmt.query_map([], map_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_by_key(&self, key: &str) -> Result<Option<JiraInboxItem>> {
        self.db
            .conn
            .query_row(
                "SELECT issue_key, issue_id, summary, status, priority, priority_rank, url,
                        first_seen, last_seen, notified, pinned, dismissed, raw_updated
                 FROM jira_inbox WHERE issue_key = ?1",
                params![key],
                map_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn set_pinned(&self, key: &str, pinned: bool) -> Result<bool> {
        let n = self.db.conn.execute(
            "UPDATE jira_inbox SET pinned = ?1 WHERE issue_key = ?2",
            params![pinned as i32, key],
        )?;
        Ok(n > 0)
    }

    pub fn set_dismissed(&self, key: &str, dismissed: bool) -> Result<bool> {
        let n = self.db.conn.execute(
            "UPDATE jira_inbox SET dismissed = ?1 WHERE issue_key = ?2",
            params![dismissed as i32, key],
        )?;
        Ok(n > 0)
    }

    pub fn mark_notified(&self, keys: &[String]) -> Result<()> {
        for key in keys {
            self.db.conn.execute(
                "UPDATE jira_inbox SET notified = 1 WHERE issue_key = ?1",
                params![key],
            )?;
        }
        Ok(())
    }

    /// Un-notified newly inserted items (for toast after sync).
    pub fn list_unnotified_new(&self, keys: &[String]) -> Result<Vec<JiraInboxItem>> {
        let mut items = Vec::new();
        for key in keys {
            if let Some(item) = self.get_by_key(key)? {
                if !item.notified && !item.dismissed {
                    items.push(item);
                }
            }
        }
        Ok(items)
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JiraInboxItem> {
    Ok(JiraInboxItem {
        issue_key: row.get(0)?,
        issue_id: row.get(1)?,
        summary: row.get(2)?,
        status: row.get(3)?,
        priority: row.get(4)?,
        priority_rank: row.get(5)?,
        url: row.get(6)?,
        first_seen: row.get(7)?,
        last_seen: row.get(8)?,
        notified: row.get::<_, i32>(9)? != 0,
        pinned: row.get::<_, i32>(10)? != 0,
        dismissed: row.get::<_, i32>(11)? != 0,
        raw_updated: row.get(12)?,
    })
}
