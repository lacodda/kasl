use crate::db::db::Db;
use crate::libs::messages::Message;
use crate::msg_error_anyhow;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const SCHEMA_TAGS: &str = "CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: Option<i32>,
    pub name: String,
    pub color: Option<String>,
    pub created_at: Option<String>,
}

impl Tag {
    pub fn new(name: String, color: Option<String>) -> Self {
        Self {
            id: None,
            name,
            color,
            created_at: None,
        }
    }
}

pub struct Tags {
    conn: Connection,
}

impl Tags {
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        // Ensure tables exist (migration v3 creates them, but we ensure here too)
        db.conn.execute(SCHEMA_TAGS, [])?;
        db.conn.execute(SCHEMA_TASK_TAGS, [])?;
        Ok(Self { conn: db.conn })
    }

    /// Create a new tag
    pub fn create(&mut self, tag: &Tag) -> Result<i32> {
        self.conn.execute(INSERT_TAG, params![tag.name, tag.color])?;
        Ok(self.conn.last_insert_rowid() as i32)
    }

    /// Update an existing tag
    pub fn update(&mut self, id: i32, name: &str, color: Option<&str>) -> Result<()> {
        let affected = self.conn.execute(UPDATE_TAG, params![id, name, color])?;
        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TagNotFound(id.to_string())));
        }
        Ok(())
    }

    /// Delete a tag
    pub fn delete(&mut self, id: i32) -> Result<()> {
        let affected = self.conn.execute(DELETE_TAG, params![id])?;
        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TagNotFound(id.to_string())));
        }
        Ok(())
    }

    /// Get all tags
    pub fn list(&mut self) -> Result<Vec<Tag>> {
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

    /// Get a tag by name
    pub fn get_by_name(&mut self, name: &str) -> Result<Option<Tag>> {
        self.conn
            .query_row(SELECT_TAG_BY_NAME, params![name], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .optional()
            .map_err(Into::into)
    }

    /// Get a tag by ID
    pub fn get_by_id(&mut self, id: i32) -> Result<Option<Tag>> {
        self.conn
            .query_row(SELECT_TAG_BY_ID, params![id], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .optional()
            .map_err(Into::into)
    }

    /// Get tags for a specific task
    pub fn get_task_tags(&mut self, task_id: i32) -> Result<Vec<Tag>> {
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

    /// Get task IDs that have a specific tag
    pub fn get_tasks_with_tag(&mut self, tag_id: i32) -> Result<Vec<i32>> {
        let mut stmt = self.conn.prepare(SELECT_TASKS_BY_TAG)?;
        let task_iter = stmt.query_map(params![tag_id], |row| row.get(0))?;

        let mut task_ids = Vec::new();
        for task_id in task_iter {
            task_ids.push(task_id?);
        }
        Ok(task_ids)
    }

    /// Add a tag to a task
    pub fn add_tag_to_task(&mut self, task_id: i32, tag_id: i32) -> Result<()> {
        self.conn.execute(INSERT_TASK_TAG, params![task_id, tag_id])?;
        Ok(())
    }

    /// Remove a tag from a task
    pub fn remove_tag_from_task(&mut self, task_id: i32, tag_id: i32) -> Result<()> {
        self.conn.execute(DELETE_TASK_TAG, params![task_id, tag_id])?;
        Ok(())
    }

    /// Remove all tags from a task
    pub fn clear_task_tags(&mut self, task_id: i32) -> Result<()> {
        self.conn.execute(DELETE_ALL_TASK_TAGS, params![task_id])?;
        Ok(())
    }

    /// Set tags for a task (replaces existing tags)
    pub fn set_task_tags(&mut self, task_id: i32, tag_ids: &[i32]) -> Result<()> {
        // Clear existing tags
        self.clear_task_tags(task_id)?;

        // Add new tags
        for tag_id in tag_ids {
            self.add_tag_to_task(task_id, *tag_id)?;
        }
        Ok(())
    }

    /// Get or create tags by names
    pub fn get_or_create_tags(&mut self, names: &[String]) -> Result<Vec<i32>> {
        let mut tag_ids = Vec::new();

        for name in names {
            let tag = match self.get_by_name(name)? {
                Some(existing_tag) => existing_tag,
                None => {
                    // Create new tag with a default color
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

    /// Get a default color for new tags
    fn get_default_color() -> String {
        // Simple color rotation
        static COLORS: &[&str] = &["blue", "green", "yellow", "red", "purple", "cyan", "orange"];
        static mut COLOR_INDEX: usize = 0;

        unsafe {
            let color = COLORS[COLOR_INDEX % COLORS.len()];
            COLOR_INDEX += 1;
            color.to_string()
        }
    }
}
