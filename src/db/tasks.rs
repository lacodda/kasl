use super::db::Db;
use crate::libs::task::Task;
use rusqlite::{params, Connection, Result};
use std::error::Error;

const SCHEMA_TASKS: &str = "CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER NOT NULL PRIMARY KEY,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    name TEXT NOT NULL,
    comment TEXT,
    completeness INT
);";
const INSERT_TASK: &str = "INSERT INTO tasks (timestamp, name, comment, completeness) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'), ?, ?, ?)";

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
}
