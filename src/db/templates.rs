use crate::db::db::Db;
use crate::libs::messages::Message;
use crate::msg_error_anyhow;
use anyhow::Result;
use rusqlite::{params, Connection};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    pub id: Option<i32>,
    pub name: String,      // Template name (unique identifier)
    pub task_name: String, // Actual task name
    pub comment: String,
    pub completeness: i32,
    pub created_at: Option<String>,
}

impl TaskTemplate {
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

pub struct Templates {
    conn: Connection,
}

impl Templates {
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        // Note: The templates table is created by migration v2, but we ensure it exists
        db.conn.execute(SCHEMA_TEMPLATES, [])?;
        Ok(Self { conn: db.conn })
    }

    /// Create a new template
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

    /// Update an existing template
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

    /// Delete a template by name
    pub fn delete(&mut self, name: &str) -> Result<()> {
        let affected = self.conn.execute(DELETE_TEMPLATE, params![name])?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TemplateNotFound(name.to_string())));
        }

        Ok(())
    }

    /// Get all templates
    pub fn list(&mut self) -> Result<Vec<TaskTemplate>> {
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

    /// Get a template by name
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

    /// Search templates by name or task name
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

    /// Check if a template exists
    pub fn exists(&mut self, name: &str) -> Result<bool> {
        Ok(self.get(name)?.is_some())
    }
}
