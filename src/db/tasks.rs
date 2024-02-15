use super::db::Db;
use crate::libs::task::{Task, TaskFilter};
use rusqlite::{params, params_from_iter, Connection, Result};
use std::{error::Error, vec};

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
const WHERE_CURRENT_DATE: &str = "WHERE DATE(timestamp) = DATE('now')";
const WHERE_ID_IN: &str = "WHERE id IN";
const WHERE_INCOMPLETE: &str = "WHERE
  completeness < 100 AND
  task_id NOT IN (SELECT task_id FROM tasks WHERE DATE(timestamp) = DATE('now')) AND
  (task_id, completeness) IN (SELECT task_id, MAX(completeness) FROM tasks
  WHERE DATE(timestamp) BETWEEN datetime(CURRENT_TIMESTAMP, 'localtime', '-15 day') AND datetime(CURRENT_TIMESTAMP, 'localtime', '-1 day')
  GROUP BY task_id)
  GROUP BY task_id";

#[derive(Debug)]
pub struct Tasks {
    pub conn: Connection,
    pub id: Option<i32>,
}

impl Tasks {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let db = Db::new()?;
        db.conn.execute(&SCHEMA_TASKS, [])?;

        Ok(Self { conn: db.conn, id: None })
    }

    pub fn insert(&mut self, task: &Task) -> Result<&mut Self, Box<dyn Error>> {
        self.id = self.conn.query_row(
            INSERT_TASK,
            params![task.task_id, task.name, task.comment, task.completeness, task.excluded_from_search],
            |row| row.get(0),
        )?;

        Ok(self)
    }

    pub fn update_id(&mut self) -> Result<&mut Self, Box<dyn Error>> {
        self.conn.execute(UPDATE_TASK_ID, params![self.id, self.id])?;

        Ok(self)
    }

    pub fn get(&mut self) -> Result<Vec<Task>, Box<dyn Error>> {
        if self.id.is_none() {
            return Err("No ID".into());
        }
        self.fetch(TaskFilter::ByIds(vec![self.id.unwrap()]))
    }

    pub fn fetch(&mut self, filter: TaskFilter) -> Result<Vec<Task>, Box<dyn Error>> {
        let (mut stmt, params) = match filter {
            TaskFilter::All => (self.conn.prepare(SELECT_TASKS)?, vec![]),
            TaskFilter::Today => (self.conn.prepare(&format!("{} {}", SELECT_TASKS, WHERE_CURRENT_DATE))?, vec![]),
            TaskFilter::Incomplete => (self.conn.prepare(&format!("{} {}", SELECT_TASKS, WHERE_INCOMPLETE))?, vec![]),
            TaskFilter::ByIds(ids) => (self.conn.prepare(&Self::query_by_ids(&ids))?, ids),
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

    fn query_by_ids(ids: &Vec<i32>) -> String {
        format!("{} {} ({})", SELECT_TASKS, WHERE_ID_IN, vec!["?"; ids.len()].join(", "))
    }
}
