use super::db::Db;
use crate::libs::task::{Task, TaskFilter};
use rusqlite::{params, params_from_iter, Connection, Result};
use std::error::Error;

const SCHEMA_TASKS: &str = "CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER NOT NULL PRIMARY KEY,
    task_id INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 0,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 100,
    excluded_from_search BOOLEAN NOT NULL ON CONFLICT REPLACE DEFAULT FALSE
);";
const INSERT_TASK: &str =
    "INSERT INTO tasks (task_id, timestamp, name, comment, completeness, excluded_from_search) VALUES (?, datetime(CURRENT_TIMESTAMP, 'localtime'), ?, ?, ?, ?)";
const SELECT_TASKS: &str = "SELECT * FROM tasks";
const WHERE_DATE: &str = "WHERE DATE(timestamp) = DATE('now')";
const WHERE_ID: &str = "WHERE id IN";

pub struct Tasks {
    pub conn: Connection,
}

impl Tasks {
    pub fn new() -> Result<Tasks, Box<dyn Error>> {
        let db = Db::new()?;
        db.conn.execute(&SCHEMA_TASKS, [])?;

        Ok(Tasks { conn: db.conn })
    }

    pub fn insert(&mut self, task: &Task) -> Result<()> {
        self.conn
            .execute(INSERT_TASK, params![task.task_id, task.name, task.comment, task.completeness, task.excluded_from_search])?;

        Ok(())
    }

    pub fn fetch(&mut self, filter: TaskFilter) -> Result<Vec<Task>, Box<dyn Error>> {
        let (mut stmt, params) = match filter {
            TaskFilter::All => (self.conn.prepare(SELECT_TASKS)?, vec![]),
            TaskFilter::Today => (self.conn.prepare(&format!("{} {}", SELECT_TASKS, WHERE_DATE))?, vec![]),
            TaskFilter::ByIds(ids) => (self.conn.prepare(&format!("{} {} ({})", SELECT_TASKS, WHERE_ID, vec!["?"; ids.len()].join(", ")))?, ids),
        };

        let task_iter = stmt.query_map(params_from_iter(params.iter()), |row| {
            Ok(Task {
                id: row.get(0)?,
                task_id: row.get(1)?,
                timestamp: row.get(2)?,
                name: row.get(3)?,
                comment: row.get(4)?,
                completeness: row.get(5)?,
                excluded_from_search: row.get(6)?,
            })
        })?;
        let mut tasks = Vec::new();
        for task_result in task_iter {
            tasks.push(task_result?);
        }

        Ok(tasks)
    }
}
