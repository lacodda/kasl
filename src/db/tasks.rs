//! Task table access: CRUD plus the filtered queries behind `TaskFilter`.
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use kasl::db::tasks::Tasks;
//! use kasl::libs::task::Task;
//!
//! let mut tasks = Tasks::new()?;
//! let task = Task::new("Review code", "Check PR #123", Some(75));
//! tasks.insert(&task)?;
//! # Ok(())
//! # }
//! ```

use super::db::Db;
use crate::libs::messages::Message;
use crate::libs::task::{Task, TaskFilter};
use crate::msg_error_anyhow;
use anyhow::Result;
use rusqlite::{Connection, Statement, ToSql, params};
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
const WHERE_DATE: &str = "WHERE date(timestamp) = date(?1)";
const WHERE_ID_IN: &str = "WHERE id IN";

// Incomplete = the latest completion state per task_id over the last 15 days,
// still under 100%, and not already re-listed today.
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

/// Task table access; remembers the last inserted id for chaining.
#[derive(Debug)]
pub struct Tasks {
    pub conn: Connection,

    /// Id of the most recently inserted task, for `update_id`/`get` chaining.
    pub id: Option<i32>,
}

impl Tasks {
    /// Opens the database and ensures the tasks table exists.
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::db::tasks::Tasks;
    ///
    /// let mut tasks = Tasks::new()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        db.conn.execute(SCHEMA_TASKS, [])?;
        Ok(Self { conn: db.conn, id: None })
    }

    /// Inserts the task, storing the assigned id for chaining.
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::db::tasks::Tasks;
    /// use kasl::libs::task::Task;
    ///
    /// let mut tasks = Tasks::new()?;
    /// let task = Task::new("Code review", "Review PR #123", Some(50));
    /// tasks.insert(&task)?
    ///      .update_id()?; // Method chaining
    /// # Ok(())
    /// # }
    /// ```
    pub fn insert(&mut self, task: &Task) -> Result<&mut Self> {
        self.id = Some(self.conn.query_row(
            INSERT_TASK,
            params![task.task_id, task.name, task.comment, task.completeness, task.excluded_from_search],
            |row| row.get(0),
        )?);

        Ok(self)
    }

    /// Points the just-inserted task's `task_id` at its own id - the
    /// convention for a standalone task that groups its own history.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tasks::Tasks;
    /// use kasl::libs::task::Task;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tasks = Tasks::new()?;
    /// let subtask = Task::new("Subtask", "Part of larger task", Some(0));
    /// tasks.insert(&subtask)?
    ///      .update_id()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn update_id(&mut self) -> Result<&mut Self> {
        self.conn.execute(UPDATE_TASK_ID, params![self.id, self.id])?;
        Ok(self)
    }

    /// Fetches the most recently inserted task.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tasks::Tasks;
    /// use kasl::libs::task::Task;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tasks = Tasks::new()?;
    /// let task = Task::new("New task", "Description", Some(100));
    /// let inserted_tasks = tasks.insert(&task)?
    ///                          .get()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get(&mut self) -> Result<Vec<Task>> {
        let id = self.id.ok_or_else(|| msg_error_anyhow!(Message::NoIdSet))?;
        self.fetch(TaskFilter::ByIds(vec![id]))
    }

    /// Runs the query for the given filter and attaches each task's tags.
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::db::tasks::Tasks;
    /// use kasl::libs::task::TaskFilter;
    /// use chrono::Local;
    ///
    /// let mut tasks = Tasks::new()?;
    ///
    /// let all_tasks = tasks.fetch(TaskFilter::All)?;
    /// let today = tasks.fetch(TaskFilter::Date(Local::now().date_naive()))?;
    /// let urgent = tasks.fetch(TaskFilter::ByTag("urgent".to_string()))?;
    /// let incomplete = tasks.fetch(TaskFilter::Incomplete)?;
    /// # Ok(())
    /// # }
    /// ```
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
                tags: vec![], // Tags will be populated in the next step
            })
        })?;

        let mut tasks = Vec::new();
        for task_result in task_iter {
            tasks.push(task_result?);
        }

        // Enrich tasks with tag information
        let mut tags_db = crate::db::tags::Tags::new()?;
        for task in &mut tasks {
            if let Some(task_id) = task.id {
                task.tags = tags_db.get_tags_by_task(task_id)?;
            }
        }

        Ok(tasks)
    }

    /// Builds `SELECT ... WHERE id IN (?, ?, ...)` with one placeholder per id.
    fn query_by_ids(ids: &[i32]) -> String {
        format!("{} {} ({})", SELECT_TASKS, WHERE_ID_IN, vec!["?"; ids.len()].join(", "))
    }

    /// Deletes one task; returns the number of rows removed.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tasks::Tasks;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tasks = Tasks::new()?;
    /// let task_id = 1;
    /// let deleted_count = tasks.delete(task_id)?;
    /// if deleted_count > 0 {
    ///     println!("Task deleted successfully");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete(&mut self, id: i32) -> Result<usize> {
        let affected = self.conn.execute(DELETE_TASK, params![id])?;
        Ok(affected)
    }

    /// Deletes several tasks in one statement; unknown ids are simply not
    /// counted, and an empty slice is a no-op.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tasks::Tasks;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tasks = Tasks::new()?;
    /// let ids_to_delete = vec![101, 102, 103];
    /// let deleted_count = tasks.delete_many(&ids_to_delete)?;
    /// println!("Deleted {} tasks", deleted_count);
    /// # Ok(())
    /// # }
    /// ```
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

    /// True when a task with this id exists.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tasks::Tasks;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tasks = Tasks::new()?;
    /// let task_id = 1;
    /// if tasks.exists(task_id)? {
    ///     println!("Task exists and can be updated");
    /// } else {
    ///     println!("Task not found");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn exists(&mut self, id: i32) -> Result<bool> {
        let count: i32 = self.conn.query_row(SELECT_COUNT_BY_ID, params![id], |row| row.get(0))?;
        Ok(count > 0)
    }

    /// Updates name, comment and completeness; identity fields and tag
    /// links stay as they are. Errors when the task has no id or no longer
    /// exists.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tasks::Tasks;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tasks = Tasks::new()?;
    /// let task_id = 1;
    /// let mut task = tasks.get_by_id(task_id)?.unwrap();
    /// task.name = "Updated task name".to_string();
    /// task.completeness = Some(75);
    /// tasks.update(&task)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn update(&mut self, task: &Task) -> Result<()> {
        let id = task.id.ok_or_else(|| msg_error_anyhow!(Message::NoIdSet))?;

        let affected = self.conn.execute(UPDATE_TASK, params![task.name, task.comment, task.completeness, id])?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TaskUpdateFailed));
        }

        Ok(())
    }

    /// Fetches one task by id, tags included.
    ///
    /// ```rust,no_run
    /// # use kasl::db::tasks::Tasks;
    /// # fn main() -> anyhow::Result<()> {
    /// let mut tasks = Tasks::new()?;
    /// if let Some(task) = tasks.get_by_id(42)? {
    ///     println!("Found task: {}", task.name);
    /// } else {
    ///     println!("Task with ID 42 not found");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_by_id(&mut self, id: i32) -> Result<Option<Task>> {
        let mut tasks = self.fetch(TaskFilter::ByIds(vec![id]))?;
        Ok(tasks.pop())
    }
}
