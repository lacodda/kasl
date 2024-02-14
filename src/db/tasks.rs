use super::db::Db;
use crate::libs::task::{Task, TaskFilter};
use rusqlite::{params, params_from_iter, Connection, Result};
use std::error::Error;

const SCHEMA_TASKS: &str = "CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER NOT NULL PRIMARY KEY,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    name TEXT NOT NULL,
    comment TEXT,
    completeness INT
);";
const INSERT_TASK: &str = "INSERT INTO tasks (timestamp, name, comment, completeness) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'), ?, ?, ?)";
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
        self.conn.execute(INSERT_TASK, params![task.name, task.comment, task.completeness])?;

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
                timestamp: row.get(1)?,
                name: row.get(2)?,
                comment: row.get(3)?,
                completeness: row.get(4)?,
            })
        })?;
        let mut tasks = Vec::new();
        for task_result in task_iter {
            tasks.push(task_result?);
        }

        Ok(tasks)
    }
}
