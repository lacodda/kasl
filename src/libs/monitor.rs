use enigo::{Enigo, Settings};
use parking_lot::Mutex;
use rusqlite::{Connection, Result};
use std::error::Error;
use std::sync::Arc;
use tokio::time::{self, Duration, Instant};

/// Defines the configuration for the activity monitor.
#[derive(Debug)]
pub struct MonitorConfig {
    pub breaks_enabled: bool,
    pub break_threshold: u64, // Inactivity duration in seconds to trigger a break.
    pub poll_interval: u64,   // Interval in milliseconds to check for activity.
}

/// Represents the activity monitor.
pub struct Monitor {
    config: MonitorConfig,
    db: Arc<Mutex<Connection>>, // Thread-safe SQLite database connection.
}

impl Monitor {
    /// Creates a new `Monitor` instance.
    ///
    /// Initializes the SQLite database connection and creates the 'breaks' table if it doesn't exist.
    ///
    /// # Arguments
    /// * `config` - The `MonitorConfig` for the monitor.
    /// * `db_path` - The path to the SQLite database file.
    pub fn new(config: MonitorConfig, db_path: &str) -> Result<Self> {
        let db_conn = Connection::open(db_path)?;
        db_conn.execute(
            "CREATE TABLE IF NOT EXISTS breaks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start TEXT NOT NULL,
                end TEXT,
                duration INTEGER
            )",
            [],
        )?;
        let db = Arc::new(Mutex::new(db_conn));
        Ok(Monitor { config, db })
    }

    /// Runs the main activity monitoring loop.
    ///
    /// This asynchronous function continuously checks for user activity and records breaks
    /// based on the configured `break_threshold` and `poll_interval`.
    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        println!("Monitor is running");
        if !self.config.breaks_enabled {
            return Ok(());
        }

        let enigo = Enigo::new(&Settings::default()).unwrap();
        let mut last_activity = Instant::now();
        let mut in_break = false;

        loop {
            let activity_detected = self.detect_activity(&enigo);

            if activity_detected {
                if in_break {
                    self.insert_break_end()?;
                    in_break = false;
                }
                last_activity = Instant::now();
            } else if !in_break && last_activity.elapsed() >= Duration::from_secs(self.config.break_threshold) {
                println!("Break Start");
                self.insert_break_start()?;
                in_break = true;
            }

            time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }

    /// Placeholder function to detect user activity.
    ///
    /// **Note:** This implementation currently always returns `false`.
    /// A real-world application would use `enigo` or other OS-specific APIs
    /// to monitor actual keyboard, mouse, or scroll events.
    fn detect_activity(&self, _enigo: &Enigo) -> bool {
        false
    }

    /// Inserts a new break start record into the database.
    ///
    /// Records the current local timestamp when a period of inactivity begins.
    fn insert_break_start(&self) -> Result<()> {
        let start = chrono::Local::now().to_rfc3339();
        let db = self.db.lock();
        db.execute("INSERT INTO breaks (start) VALUES (?1)", [&start])?;
        Ok(())
    }

    /// Updates the most recently started break record with an end timestamp and its duration.
    ///
    /// Finds the last break where the `end` time is `NULL`, updates it with the current time,
    /// and calculates the duration in seconds.
    fn insert_break_end(&self) -> Result<(), Box<dyn Error>> {
        let end = chrono::Local::now();
        let end_str = end.to_rfc3339();
        let db = self.db.lock();

        let mut stmt = db.prepare("SELECT id, start FROM breaks WHERE end IS NULL ORDER BY id DESC LIMIT 1")?;
        let break_row = stmt.query_row([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)));

        if let Ok((id, start_str)) = break_row {
            let start = chrono::DateTime::parse_from_rfc3339(&start_str)?.with_timezone(&chrono::Local);
            let duration = (end - start).num_seconds();
            db.execute(
                "UPDATE breaks SET end = ?1, duration = ?2 WHERE id = ?3",
                [&end_str, &duration.to_string(), &id.to_string()],
            )?;
        }
        Ok(())
    }
}
