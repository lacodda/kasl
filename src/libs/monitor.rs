use crate::db::breaks::Breaks;
use enigo::{Enigo, Settings};
use std::error::Error;
use tokio::time::{self, Duration, Instant};

// Defines the configuration for the activity monitor.
#[derive(Debug)]
pub struct MonitorConfig {
    pub breaks_enabled: bool,
    pub break_threshold: u64, // Inactivity duration in seconds to trigger a break.
    pub poll_interval: u64,   // Interval in milliseconds to check for activity.
}

// Represents the activity monitor.
pub struct Monitor {
    config: MonitorConfig,
    breaks: Breaks, // Manages break database operations.
}

impl Monitor {
    // Creates a new Monitor instance.
    //
    // Initializes the Breaks module for database operations.
    //
    // # Arguments
    // * `config` - The MonitorConfig for the monitor.
    pub fn new(config: MonitorConfig) -> Result<Self, Box<dyn Error>> {
        let breaks = Breaks::new()?; // Breaks::new() handles the connection now
        Ok(Monitor { config, breaks })
    }

    // Runs the main activity monitoring loop.
    //
    // This asynchronous function continuously checks for user activity and records breaks
    // based on the configured break_threshold and poll_interval.
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

    // Placeholder function to detect user activity.
    //
    // Note: This implementation currently always returns false.
    // A real-world application would use enigo or other OS-specific APIs
    // to monitor actual keyboard, mouse, or scroll events.
    fn detect_activity(&self, _enigo: &Enigo) -> bool {
        false
    }

    // Inserts a new break start record into the database.
    fn insert_break_start(&self) -> Result<(), Box<dyn Error>> {
        self.breaks.insert_start()?;
        Ok(())
    }

    // Updates the most recently started break record with an end timestamp and duration.
    fn insert_break_end(&self) -> Result<(), Box<dyn Error>> {
        self.breaks.insert_end()?;
        Ok(())
    }
}
