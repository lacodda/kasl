use super::db::Db;
use crate::libs::messages::Message;
use crate::libs::task::{Task, TaskFilter};
use crate::msg_error_anyhow;
use anyhow::Result;
use rusqlite::{params, Connection, Statement, ToSql};
use std::vec;

const SCHEMA_TASKS: &str = "CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER NOT NULL PRIMARY KEY,
    task_id INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 0,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 100,
    excluded_from_search BOOLEAN NOT NULL ON CONFLICT REPLACE DEFAULT FALSE
);";
const INSERT_TASK: &str = "INSERT INTO tasks (task_id, timestamp, name, comment, completeness, excluded_from_search) VALUES
    (?, datetime(CURRENT_TIMESTAMP, 'localtime'), ?, ?, ?, ?) RETURNING id";
const UPDATE_TASK_ID: &str = "UPDATE tasks SET task_id = ? WHERE id = ?";
const SELECT_TASKS: &str = "SELECT * FROM tasks";
const WHERE_DATE: &str = "WHERE date(timestamp) = date(?1, 'localtime')";
const WHERE_ID_IN: &str = "WHERE task_id IN";
const WHERE_INCOMPLETE: &str = "WHERE
  completeness < 100 AND
  task_id NOT IN (SELECT task_id FROM tasks WHERE DATE(timestamp) = DATE('now')) AND
  (task_id, completeness) IN (SELECT task_id, MAX(completeness) FROM tasks
  WHERE DATE(timestamp) BETWEEN datetime(CURRENT_TIMESTAMP, 'localtime', '-15 day') AND datetime(CURRENT_TIMESTAMP, 'localtime', '-1 day')
  GROUP BY task_id)
  GROUP BY task_id";
const WHERE_TAG: &str = "WHERE id IN (SELECT task_id FROM task_tags tt JOIN tags t ON tt.tag_id = t.id WHERE t.name = ?1)";
const WHERE_TAGS: &str = "WHERE id IN (SELECT task_id FROM task_tags tt JOIN tags t ON tt.tag_id = t.id WHERE t.name IN";
const DELETE_TASK: &str = "DELETE FROM tasks WHERE id = ?";
const DELETE_TASKS_BY_IDS: &str = "DELETE FROM tasks WHERE id IN";
const SELECT_COUNT_BY_ID: &str = "SELECT COUNT(*) FROM tasks WHERE id = ?";
const UPDATE_TASK: &str = "UPDATE tasks SET name = ?, comment = ?, completeness = ? WHERE id = ?";

#[derive(Debug)]
pub struct Tasks {
    pub conn: Connection,
    pub id: Option<i32>,
}

impl Tasks {
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        db.conn.execute(&SCHEMA_TASKS, [])?;

        Ok(Self { conn: db.conn, id: None })
    }

    pub fn insert(&mut self, task: &Task) -> Result<&mut Self> {
        self.id = Some(self.conn.query_row(
            INSERT_TASK,
            params![task.task_id, task.name, task.comment, task.completeness, task.excluded_from_search],
            |row| row.get(0),
        )?);

        Ok(self)
    }

    pub fn update_id(&mut self) -> Result<&mut Self> {
        self.conn.execute(UPDATE_TASK_ID, params![self.id, self.id])?;

        Ok(self)
    }

    pub fn get(&mut self) -> Result<Vec<Task>> {
        let id = self.id.ok_or_else(|| msg_error_anyhow!(Message::NoIdSet))?;
        self.fetch(TaskFilter::ByIds(vec![id]))
    }

    pub fn fetch(&mut self, filter: TaskFilter) -> Result<Vec<Task>> {
        let (mut stmt, params): (Statement, Vec<Box<dyn ToSql>>) = match filter {
            TaskFilter::All => (self.conn.prepare(SELECT_TASKS)?, vec![]),
            TaskFilter::Date(date) => (self.conn.prepare(&format!("{} {}", SELECT_TASKS, WHERE_DATE))?, vec![Box::new(date)]),
            TaskFilter::Incomplete => (self.conn.prepare(&format!("{} {}", SELECT_TASKS, WHERE_INCOMPLETE))?, vec![]),
            TaskFilter::ByIds(ids) => {
                let ids_params: Vec<Box<dyn ToSql>> = ids.clone().into_iter().map(|id| Box::new(id) as Box<dyn ToSql>).collect();
                (self.conn.prepare(&Self::query_by_ids(&ids))?, ids_params)
            }
            TaskFilter::ByTag(tag_name) => (self.conn.prepare(&format!("{} {}", SELECT_TASKS, WHERE_TAG))?, vec![Box::new(tag_name)]),
            TaskFilter::ByTags(tag_names) => {
                let placeholders = vec!["?"; tag_names.len()].join(", ");
                let query = format!("{} {} ({}))", SELECT_TASKS, WHERE_TAGS, placeholders);
                let params: Vec<Box<dyn ToSql>> = tag_names.into_iter().map(|name| Box::new(name) as Box<dyn ToSql>).collect();
                (self.conn.prepare(&query)?, params)
            }
        };

        let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| &**p).collect();
        let task_iter = stmt.query_map(&params_refs[..], |row| {
            Ok(Task {
                id: row.get(0)?,
                task_id: row.get(1)?,
                timestamp: row.get(2)?,
                name: row.get(3)?,
                comment: row.get(4)?,
                completeness: row.get(5)?,
                excluded_from_search: row.get(6)?,
                tags: vec![],
            })
        })?;
        let mut tasks = Vec::new();
        for task_result in task_iter {
            tasks.push(task_result?);
        }

        let mut tags_db = crate::db::tags::Tags::new()?;
        for task in &mut tasks {
            if let Some(task_id) = task.id {
                task.tags = tags_db.get_task_tags(task_id)?;
            }
        }

        Ok(tasks)
    }

    fn query_by_ids(ids: &Vec<i32>) -> String {
        format!("{} {} ({})", SELECT_TASKS, WHERE_ID_IN, vec!["?"; ids.len()].join(", "))
    }

    /// Delete a single task by ID
    pub fn delete(&mut self, id: i32) -> Result<usize> {
        let affected = self.conn.execute(DELETE_TASK, params![id])?;
        Ok(affected)
    }

    /// Delete multiple tasks by IDs
    pub fn delete_many(&mut self, ids: &[i32]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let placeholders = vec!["?"; ids.len()].join(", ");
        let query = format!("{} ({})", DELETE_TASKS_BY_IDS, placeholders);

        let params: Vec<Box<dyn ToSql>> = ids.iter().map(|id| Box::new(*id) as Box<dyn ToSql>).collect();
        let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| &**p).collect();

        let affected = self.conn.execute(&query, &params_refs[..])?;
        Ok(affected)
    }

    /// Check if a task exists
    pub fn exists(&mut self, id: i32) -> Result<bool> {
        let count: i32 = self.conn.query_row(SELECT_COUNT_BY_ID, params![id], |row| row.get(0))?;
        Ok(count > 0)
    }

    /// Update an existing task
    pub fn update(&mut self, task: &Task) -> Result<()> {
        let id = task.id.ok_or_else(|| msg_error_anyhow!(Message::NoIdSet))?;

        let affected = self.conn.execute(UPDATE_TASK, params![task.name, task.comment, task.completeness, id])?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TaskUpdateFailed));
        }

        Ok(())
    }

    /// Fetch a single task by ID
    pub fn get_by_id(&mut self, id: i32) -> Result<Option<Task>> {
        let mut tasks = self.fetch(TaskFilter::ByIds(vec![id]))?;
        Ok(tasks.pop())
    }
}
