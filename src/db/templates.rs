//! Reusable task templates: named blueprints tasks are created from.
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use kasl::db::templates::{Templates, TaskTemplate};
//!
//! let mut templates = Templates::new()?;
//! let template = TaskTemplate::new(
//!     "daily-standup".to_string(),
//!     "Prepare for daily standup".to_string(),
//!     "Review yesterday's work and plan today".to_string(),
//!     50
//! );
//! templates.create(&template)?;
//! # Ok(())
//! # }
//! ```

use crate::db::db::Db;
use crate::libs::messages::Message;
use crate::msg_error_anyhow;
use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

const SCHEMA_TEMPLATES: &str = "CREATE TABLE IF NOT EXISTS task_templates (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    task_name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER DEFAULT 100,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";

const INSERT_TEMPLATE: &str = "INSERT INTO task_templates (name, task_name, comment, completeness) VALUES (?1, ?2, ?3, ?4)";
const UPDATE_TEMPLATE: &str = "UPDATE task_templates SET task_name = ?2, comment = ?3, completeness = ?4 WHERE name = ?1";
const DELETE_TEMPLATE: &str = "DELETE FROM task_templates WHERE name = ?1";
const SELECT_ALL_TEMPLATES: &str = "SELECT * FROM task_templates ORDER BY name";
const SELECT_TEMPLATE_BY_NAME: &str = "SELECT * FROM task_templates WHERE name = ?1";
const SEARCH_TEMPLATES: &str = "SELECT * FROM task_templates WHERE name LIKE ?1 OR task_name LIKE ?1 ORDER BY name";

/// A task blueprint. `name` identifies the template ("code-review");
/// `task_name` becomes the created task's title ("Review PR #123").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    /// Database primary key; `None` until saved.
    pub id: Option<i32>,

    /// Unique template name users type to select it.
    pub name: String,

    /// Title given to tasks created from this template.
    pub task_name: String,

    /// Default notes for created tasks.
    pub comment: String,

    /// Default completeness for created tasks.
    pub completeness: i32,

    /// Set by the database on insert.
    pub created_at: Option<String>,
}

impl TaskTemplate {
    /// Builds an unsaved template; id and timestamp are assigned on `create`.
    ///
    /// ```rust
    /// use kasl::db::templates::TaskTemplate;
    ///
    /// let template = TaskTemplate::new(
    ///     "morning-routine".to_string(),
    ///     "Complete morning routine".to_string(),
    ///     "Check emails, review calendar, plan day".to_string(),
    ///     25 // Start at 25% since some prep is already done
    /// );
    /// # let _ = template;
    /// ```
    pub fn new(name: String, task_name: String, comment: String, completeness: i32) -> Self {
        Self {
            id: None,
            name,
            task_name,
            comment,
            completeness,
            created_at: None,
        }
    }
}

/// Template table access.
pub struct Templates {
    conn: Connection,
}

impl Templates {
    /// Opens the database and ensures the templates table exists
    /// (migration v2 creates it officially).
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::db::templates::Templates;
    ///
    /// let mut templates = Templates::new()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        db.conn.execute(SCHEMA_TEMPLATES, [])?;
        Ok(Templates { conn: db.conn })
    }

    /// Inserts the template; the unique name constraint rejects duplicates.
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::db::templates::{Templates, TaskTemplate};
    ///
    /// let mut templates = Templates::new()?;
    /// let template = TaskTemplate::new(
    ///     "weekly-review".to_string(),
    ///     "Weekly review and planning".to_string(),
    ///     "Review accomplishments and plan next week".to_string(),
    ///     0
    /// );
    /// templates.create(&template)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create(&mut self, template: &TaskTemplate) -> Result<()> {
        let affected = self.conn.execute(
            INSERT_TEMPLATE,
            params![template.name, template.task_name, template.comment, template.completeness],
        )?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TemplateCreateFailed));
        }

        Ok(())
    }

    /// Updates the template's content by name; errors if it does not exist.
    pub fn update(&mut self, template: &TaskTemplate) -> Result<()> {
        let affected = self.conn.execute(
            UPDATE_TEMPLATE,
            params![template.name, template.task_name, template.comment, template.completeness],
        )?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TemplateNotFound(template.name.clone())));
        }

        Ok(())
    }

    /// Deletes the template by name; tasks created from it are unaffected.
    ///
    /// ```rust,no_run
    /// # use kasl::db::templates::Templates;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut templates = Templates::new()?;
    /// templates.delete("obsolete-template")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete(&mut self, name: &str) -> Result<()> {
        let affected = self.conn.execute(DELETE_TEMPLATE, params![name])?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TemplateNotFound(name.to_string())));
        }

        Ok(())
    }

    /// Returns every template, sorted by name.
    ///
    /// ```rust,no_run
    /// # use kasl::db::templates::Templates;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut templates = Templates::new()?;
    /// let all_templates = templates.get_all()?;
    /// for template in all_templates {
    ///     println!("Template: {} -> {}", template.name, template.task_name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_all(&mut self) -> Result<Vec<TaskTemplate>> {
        let mut stmt = self.conn.prepare(SELECT_ALL_TEMPLATES)?;
        let template_iter = stmt.query_map([], |row| {
            Ok(TaskTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                task_name: row.get(2)?,
                comment: row.get(3)?,
                completeness: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut templates = Vec::new();
        for template in template_iter {
            templates.push(template?);
        }
        Ok(templates)
    }

    /// Fetches one template by exact name; `search` is the fuzzy variant.
    ///
    /// ```rust,no_run
    /// # use kasl::db::templates::Templates;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut templates = Templates::new()?;
    /// if let Some(template) = templates.get("daily-standup")? {
    ///     println!("Found template: {}", template.task_name);
    /// } else {
    ///     println!("Template 'daily-standup' not found");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get(&mut self, name: &str) -> Result<Option<TaskTemplate>> {
        let mut stmt = self.conn.prepare(SELECT_TEMPLATE_BY_NAME)?;
        let mut template_iter = stmt.query_map(params![name], |row| {
            Ok(TaskTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                task_name: row.get(2)?,
                comment: row.get(3)?,
                completeness: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        match template_iter.next() {
            Some(Ok(template)) => Ok(Some(template)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Substring search over template names and task names, sorted by name.
    ///
    /// ```rust,no_run
    /// # use kasl::db::templates::Templates;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut templates = Templates::new()?;
    ///
    /// let review_templates = templates.search("review")?;
    /// let standup_templates = templates.search("standup")?;
    ///
    /// for template in review_templates {
    ///     println!("Found: {} -> {}", template.name, template.task_name);
    /// }
    /// # let _ = standup_templates;
    /// # Ok(())
    /// # }
    /// ```
    pub fn search(&mut self, query: &str) -> Result<Vec<TaskTemplate>> {
        let search_pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(SEARCH_TEMPLATES)?;

        let template_iter = stmt.query_map(params![search_pattern], |row| {
            Ok(TaskTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                task_name: row.get(2)?,
                comment: row.get(3)?,
                completeness: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut templates = Vec::new();
        for template in template_iter {
            templates.push(template?);
        }

        Ok(templates)
    }

    /// True when a template with this exact name exists.
    ///
    /// ```rust,no_run
    /// # use kasl::db::templates::{Templates, TaskTemplate};
    /// # fn main() -> anyhow::Result<()> {
    /// let mut templates = Templates::new()?;
    /// if templates.exists("daily-standup")? {
    ///     println!("Template already exists");
    /// } else {
    ///     let template = TaskTemplate::new(
    ///         "daily-standup".to_string(),
    ///         "Prepare for daily standup".to_string(),
    ///         "Review progress and plan".to_string(),
    ///         0
    ///     );
    ///     templates.create(&template)?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn exists(&mut self, name: &str) -> Result<bool> {
        Ok(self.get(name)?.is_some())
    }
}
