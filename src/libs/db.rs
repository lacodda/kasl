use crate::commands::{event::Event, task::Task};
use rusqlite::{params, Connection, Result};
use std::error::Error;

// EVENTS
const SCHEMA_EVENTS: &str = "CREATE TABLE IF NOT EXISTS events (
    id INTEGER NOT NULL PRIMARY KEY,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    event_type VARCHAR(32) NOT NULL
);";
const INSERT_EVENT: &str = "INSERT INTO events (timestamp, event_type) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'), ?)";

// TASKS
const SCHEMA_TASKS: &str = "CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER NOT NULL PRIMARY KEY,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    name TEXT NOT NULL,
    comment TEXT,
    completeness INT
);";
const INSERT_TASK: &str = "INSERT INTO tasks (timestamp, name, comment, completeness) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'), ?, ?, ?)";

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn new() -> Result<Db, Box<dyn Error>> {
        let conn: Connection = Connection::open("wflow.db")?;
        conn.execute(&SCHEMA_EVENTS, [])?;
        conn.execute(&SCHEMA_TASKS, [])?;

        Ok(Db { conn })
    }

    pub fn insert_event(&mut self, event: &Event) -> Result<()> {
        let event_type: String = event.event_type.to_string();
        self.conn.execute(INSERT_EVENT, params![event_type])?;

        Ok(())
    }

    pub fn insert_task(&mut self, task: &Task) -> Result<()> {
        self.conn.execute(INSERT_TASK, params![task.name, task.comment, task.completeness])?;

        Ok(())
    }
}
