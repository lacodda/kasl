//! Tags and the task_tags junction table linking them to tasks.
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use kasl::db::tags::{Tags, Tag};
//!
//! let mut tags = Tags::new()?;
//! let urgent_tag = Tag::new("urgent".to_string(), Some("red".to_string()));
//! let tag_id = tags.create(&urgent_tag)?;
//! let task_id = 1;
//! tags.add_tag_to_task(task_id, tag_id)?;
//! # Ok(())
//! # }
//! ```

use crate::db::db::Db;
use crate::libs::messages::Message;
use crate::msg_error_anyhow;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

const SCHEMA_TAGS: &str = "CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";

// Composite primary key plus ON DELETE CASCADE on both sides: removing a
// task or a tag cleans its links automatically.
const SCHEMA_TASK_TAGS: &str = "CREATE TABLE IF NOT EXISTS task_tags (
    task_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    PRIMARY KEY (task_id, tag_id),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
)";

const INSERT_TAG: &str = "INSERT INTO tags (name, color) VALUES (?1, ?2)";
const UPDATE_TAG: &str = "UPDATE tags SET name = ?2, color = ?3 WHERE id = ?1";
const DELETE_TAG: &str = "DELETE FROM tags WHERE id = ?1";
const SELECT_ALL_TAGS: &str = "SELECT * FROM tags ORDER BY name";
const SELECT_TAG_BY_NAME: &str = "SELECT * FROM tags WHERE name = ?1";
const SELECT_TAG_BY_ID: &str = "SELECT * FROM tags WHERE id = ?1";
const SELECT_TAGS_BY_TASK: &str = "
    SELECT t.* FROM tags t
    JOIN task_tags tt ON t.id = tt.tag_id
    WHERE tt.task_id = ?1
    ORDER BY t.name
";
const SELECT_TASKS_BY_TAG: &str = "SELECT task_id FROM task_tags WHERE tag_id = ?1";
const INSERT_TASK_TAG: &str = "INSERT OR IGNORE INTO task_tags (task_id, tag_id) VALUES (?1, ?2)";
const DELETE_TASK_TAG: &str = "DELETE FROM task_tags WHERE task_id = ?1 AND tag_id = ?2";
const DELETE_ALL_TASK_TAGS: &str = "DELETE FROM task_tags WHERE task_id = ?1";

/// A label attachable to tasks, with an optional display color.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    /// Database primary key; `None` until saved.
    pub id: Option<i32>,

    /// Unique, case-sensitive name.
    pub name: String,

    /// Display color (name or hex), if any.
    pub color: Option<String>,

    /// Set by the database on insert.
    pub created_at: Option<String>,
}

impl Tag {
    /// Builds an unsaved tag; id and timestamp are assigned on `create`.
    ///
    /// ```rust
    /// use kasl::db::tags::Tag;
    ///
    /// let urgent_tag = Tag::new("urgent".to_string(), Some("red".to_string()));
    /// let general_tag = Tag::new("general".to_string(), None);
    /// ```
    pub fn new(name: String, color: Option<String>) -> Self {
        Self {
            id: None,
            name,
            color,
            created_at: None,
        }
    }
}

/// Tag table access, including task-tag links.
pub struct Tags {
    conn: Connection,
}

impl Tags {
    /// Opens the database and ensures both tag tables exist
    /// (migration v3 creates them officially).
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::db::tags::Tags;
    ///
    /// let mut tags = Tags::new()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        db.conn.execute(SCHEMA_TAGS, [])?;
        db.conn.execute(SCHEMA_TASK_TAGS, [])?;
        Ok(Tags { conn: db.conn })
    }

    /// Inserts the tag and returns its assigned id; duplicate names are
    /// rejected by the unique constraint.
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::db::tags::{Tags, Tag};
    ///
    /// let mut tags = Tags::new()?;
    /// let tag = Tag::new("priority".to_string(), Some("orange".to_string()));
    /// let tag_id = tags.create(&tag)?;
    /// println!("Created tag with ID: {}", tag_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn create(&mut self, tag: &Tag) -> Result<i32> {
        self.conn.execute(INSERT_TAG, params![tag.name, tag.color])?;
        Ok(self.conn.last_insert_rowid() as i32)
    }

    /// Updates name and color by id; errors when the tag has no id or no
    /// longer exists.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let tag_id = 1;
    /// let mut tag = tags.get_by_id(tag_id)?.unwrap();
    /// tag.name = "high-priority".to_string();
    /// tag.color = Some("crimson".to_string());
    /// tags.update(&tag)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn update(&mut self, tag: &Tag) -> Result<()> {
        let id = tag.id.ok_or_else(|| msg_error_anyhow!(Message::TagNotFound(tag.name.to_string())))?;

        let affected = self.conn.execute(UPDATE_TAG, params![id, tag.name, tag.color])?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TagNotFound(tag.name.to_string())));
        }

        Ok(())
    }

    /// Deletes the tag; its task links go with it via CASCADE.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let tag_id = 1;
    /// tags.delete(tag_id)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete(&mut self, id: i32) -> Result<()> {
        let affected = self.conn.execute(DELETE_TAG, params![id])?;
        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TagNotFound(id.to_string())));
        }
        Ok(())
    }

    /// Returns every tag, sorted by name.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let all_tags = tags.get_all()?;
    /// for tag in all_tags {
    ///     println!("Tag: {} ({})", tag.name, tag.color.unwrap_or("no color".to_string()));
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_all(&mut self) -> Result<Vec<Tag>> {
        let mut stmt = self.conn.prepare(SELECT_ALL_TAGS)?;
        let tag_iter = stmt.query_map([], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut tags = Vec::new();
        for tag in tag_iter {
            tags.push(tag?);
        }
        Ok(tags)
    }

    /// Fetches one tag by exact (case-sensitive) name.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// if let Some(tag) = tags.get_by_name("urgent")? {
    ///     println!("Found tag: {} with color: {:?}", tag.name, tag.color);
    /// } else {
    ///     println!("Tag 'urgent' not found");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_by_name(&mut self, name: &str) -> Result<Option<Tag>> {
        let tag = self
            .conn
            .query_row(SELECT_TAG_BY_NAME, params![name], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .optional()?;
        Ok(tag)
    }

    /// Fetches one tag by id.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// if let Some(tag) = tags.get_by_id(42)? {
    ///     println!("Tag ID 42: {}", tag.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_by_id(&mut self, id: i32) -> Result<Option<Tag>> {
        let tag = self
            .conn
            .query_row(SELECT_TAG_BY_ID, params![id], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .optional()?;
        Ok(tag)
    }

    /// Returns the task's tags, sorted by name; empty when it has none.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let task_id = 1;
    /// let task_tags = tags.get_tags_by_task(task_id)?;
    /// for tag in task_tags {
    ///     println!("Task has tag: {}", tag.name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_tags_by_task(&mut self, task_id: i32) -> Result<Vec<Tag>> {
        let mut stmt = self.conn.prepare(SELECT_TAGS_BY_TASK)?;
        let tag_iter = stmt.query_map(params![task_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut tags = Vec::new();
        for tag in tag_iter {
            tags.push(tag?);
        }
        Ok(tags)
    }

    /// Returns the ids of tasks carrying the tag.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let tag_id = 1;
    /// let task_ids = tags.get_tasks_by_tag(tag_id)?;
    /// println!("Tag is used by {} tasks", task_ids.len());
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_tasks_by_tag(&mut self, tag_id: i32) -> Result<Vec<i32>> {
        let mut stmt = self.conn.prepare(SELECT_TASKS_BY_TAG)?;
        let task_iter = stmt.query_map(params![tag_id], |row| row.get(0))?;

        let mut task_ids = Vec::new();
        for task_id in task_iter {
            task_ids.push(task_id?);
        }
        Ok(task_ids)
    }

    /// Links a tag to a task; idempotent thanks to `OR IGNORE`.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let (task_id, tag_id) = (1, 1);
    /// tags.add_tag_to_task(task_id, tag_id)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_tag_to_task(&mut self, task_id: i32, tag_id: i32) -> Result<()> {
        self.conn.execute(INSERT_TASK_TAG, params![task_id, tag_id])?;
        Ok(())
    }

    /// Unlinks a tag from a task; a missing link is not an error.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let (task_id, tag_id) = (1, 1);
    /// tags.remove_tag_from_task(task_id, tag_id)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn remove_tag_from_task(&mut self, task_id: i32, tag_id: i32) -> Result<()> {
        self.conn.execute(DELETE_TASK_TAG, params![task_id, tag_id])?;
        Ok(())
    }

    /// Removes every tag link from the task; returns how many went.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let task_id = 1;
    /// let removed_count = tags.remove_all_tags_from_task(task_id)?;
    /// println!("Removed {} tag associations", removed_count);
    /// # Ok(())
    /// # }
    /// ```
    pub fn remove_all_tags_from_task(&mut self, task_id: i32) -> Result<usize> {
        let affected = self.conn.execute(DELETE_ALL_TASK_TAGS, params![task_id])?;
        Ok(affected)
    }

    /// Replaces the task's tag set: clears existing links, then adds the
    /// given ids. Not transactional - a failure after the clear leaves the
    /// task untagged. Ids must exist; use [`Tags::get_or_create_tags`] when
    /// unsure.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let task_id = 1;
    ///
    /// let new_tag_ids = vec![1, 3, 5]; // urgent, backend, review
    /// tags.set_task_tags(task_id, &new_tag_ids)?;
    ///
    /// let current_tags = tags.get_tags_by_task(task_id)?;
    /// assert_eq!(current_tags.len(), 3);
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_task_tags(&mut self, task_id: i32, tag_ids: &[i32]) -> Result<()> {
        self.remove_all_tags_from_task(task_id)?;

        for tag_id in tag_ids {
            self.add_tag_to_task(task_id, *tag_id)?;
        }

        Ok(())
    }

    /// Resolves names to tag ids, creating missing tags with a color from
    /// the default rotation.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tags::Tags;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tags = Tags::new()?;
    /// let tag_names = vec!["urgent".to_string(), "backend".to_string()];
    /// let tag_ids = tags.get_or_create_tags(&tag_names)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_or_create_tags(&mut self, names: &[String]) -> Result<Vec<i32>> {
        let mut tag_ids = Vec::new();

        for name in names {
            let tag = match self.get_by_name(name)? {
                Some(existing_tag) => existing_tag,
                None => {
                    let tag = Tag::new(name.clone(), Some(Self::get_default_color()));
                    let id = self.create(&tag)?;
                    Tag {
                        id: Some(id),
                        name: name.clone(),
                        color: tag.color,
                        created_at: None,
                    }
                }
            };

            if let Some(id) = tag.id {
                tag_ids.push(id);
            }
        }

        Ok(tag_ids)
    }

    /// Next color from a fixed palette, so consecutively created tags look
    /// distinct without anyone choosing.
    fn get_default_color() -> String {
        static COLORS: &[&str] = &["blue", "green", "yellow", "red", "purple", "cyan", "orange"];
        static COLOR_INDEX: AtomicUsize = AtomicUsize::new(0);

        let index = COLOR_INDEX.fetch_add(1, Ordering::Relaxed);
        COLORS[index % COLORS.len()].to_string()
    }
}
