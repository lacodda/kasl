use crate::db::breaks::Breaks;
use crate::libs::config::MonitorConfig;
use rdev::{listen, EventType};
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration, Instant};

// Represents the activity monitor.
pub struct Monitor {
    config: MonitorConfig,
    breaks: Breaks,                     // Manages break database operations.
    last_activity: Arc<Mutex<Instant>>, // Thread-safe last activity timestamp.
}

impl Monitor {
    // Creates a new Monitor instance.
    //
    // Initializes the Breaks module and starts the activity listener thread.
    //
    // # Arguments
    // * `config` - The MonitorConfig for the monitor.
    pub fn new(config: MonitorConfig) -> Result<Self, Box<dyn Error>> {
        let breaks = Breaks::new()?;
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let last_activity_clone = Arc::clone(&last_activity);

        // Start a thread to listen for input events using rdev.
        std::thread::spawn(move || {
            if let Err(e) = listen(move |event| match event.event_type {
                EventType::KeyPress(_)
                | EventType::KeyRelease(_)
                | EventType::ButtonPress(_)
                | EventType::ButtonRelease(_)
                | EventType::MouseMove { .. }
                | EventType::Wheel { .. } => {
                    *last_activity_clone.lock().unwrap() = Instant::now();
                }
            }) {
                eprintln!("Error in rdev listener: {:?}", e);
            }
        });

        Ok(Monitor { config, breaks, last_activity })
    }

    // Runs the main activity monitoring loop.
    //
    // Continuously checks for user activity and records breaks based on the configured
    // break_threshold and poll_interval.
    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        println!(
            "Monitor is running with threshold {}s, poll interval {}ms",
            self.config.break_threshold, self.config.poll_interval
        );
        if self.config.break_threshold == 0 {
            return Ok(());
        }

        let mut in_break = false;

        loop {
            let activity_detected = self.detect_activity();

            if activity_detected {
                if in_break {
                    self.insert_break_end()?;
                    in_break = false;
                }
            } else if !in_break && self.last_activity.lock().unwrap().elapsed() >= Duration::from_secs(self.config.break_threshold) {
                self.insert_break_start()?;
                in_break = true;
            }

            time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }

    // Checks if user activity has occurred since the last poll.
    //
    // Returns true if activity was detected within the poll_interval.
    fn detect_activity(&self) -> bool {
        let elapsed = self.last_activity.lock().unwrap().elapsed();
        elapsed < Duration::from_millis(self.config.poll_interval)
    }

    // Inserts a new break start record into the database.
    fn insert_break_start(&self) -> Result<(), Box<dyn Error>> {
        println!("Break Start");
        self.breaks.insert_start()?;
        Ok(())
    }

    // Updates the most recently started break record with an end timestamp and duration.
    fn insert_break_end(&self) -> Result<(), Box<dyn Error>> {
        println!("Break End");
        self.breaks.insert_end()?;
        Ok(())
    }
}
