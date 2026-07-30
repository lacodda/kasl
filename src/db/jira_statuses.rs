//! Local catalog of Jira workflow statuses (id → name).
//!
//! Populated from issue sync so inbox rows can store `status_id` and resolve
//! the display name via join.

use crate::db::db::Db;
use anyhow::Result;
use rusqlite::params;

/// Database operations for the `jira_statuses` table.
pub struct JiraStatuses {
    db: Db,
}

impl JiraStatuses {
    pub fn new() -> Result<Self> {
        Ok(Self { db: Db::new()? })
    }

    /// Inserts or updates a status name for the given Jira status id.
    pub fn upsert(&self, id: &str, name: &str) -> Result<()> {
        if id.is_empty() {
            return Ok(());
        }
        self.db.conn.execute(
            "INSERT INTO jira_statuses (id, name) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name",
            params![id, name],
        )?;
        Ok(())
    }
}
