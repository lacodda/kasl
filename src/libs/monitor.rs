use crate::db::breaks::Breaks;
use rdev::{listen, Event, EventType};
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration, Instant};

// Defines the configuration for the activity monitor.
#[derive(Debug)]
pub struct MonitorConfig {
    pub breaks_enabled: bool,
    pub break_threshold: u64,    // Inactivity duration in seconds to trigger a break.
    pub poll_interval: u64,      // Interval in milliseconds to check for activity.
    pub activity_threshold: u64, // Threshold in seconds to consider recent activity as valid.
}

// Represents the activity monitor.
pub struct Monitor {
    config: MonitorConfig,
    breaks: Breaks,                     // Manages break database operations.
    last_activity: Arc<Mutex<Instant>>, // Tracks the time of the last user activity.
}

impl Monitor {
    // Creates a new Monitor instance.
    //
    // Initializes the Breaks module and sets up activity tracking.
    //
    // # Arguments
    // * `config` - The MonitorConfig for the monitor.
    pub fn new(config: MonitorConfig) -> Result<Self, Box<dyn Error>> {
        let breaks = Breaks::new()?;
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        Ok(Monitor { config, breaks, last_activity })
    }

    // Runs the main activity monitoring loop.
    //
    // Spawns a separate thread to listen for keyboard, mouse, and scroll events using rdev.
    // Restarts the listener on error to ensure continuous monitoring.
    // Records breaks based on the configured break_threshold and poll_interval.
    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        println!("Monitor is running");
        if !self.config.breaks_enabled {
            return Ok(());
        }

        let shared_last_activity = self.last_activity.clone();
        std::thread::spawn(move || {
            loop {
                let last_activity_for_listener = shared_last_activity.clone();
                if let Err(e) = listen(move |event: Event| match event.event_type {
                    EventType::KeyPress(key) => {
                        println!("KeyPress event: {:?}", key);
                        *last_activity_for_listener.lock().unwrap() = Instant::now();
                    }
                    EventType::ButtonPress(button) => {
                        println!("ButtonPress event: {:?}", button);
                        *last_activity_for_listener.lock().unwrap() = Instant::now();
                    }
                    EventType::Wheel { delta_x, delta_y } => {
                        println!("Wheel event: delta_x={}, delta_y={}", delta_x, delta_y);
                        *last_activity_for_listener.lock().unwrap() = Instant::now();
                    }
                    _ => {}
                }) {
                    eprintln!("Failed to listen for events: {:?}. Retrying in 1 second...", e);
                    std::thread::sleep(Duration::from_secs(1));
                } else {
                    // If listen returns without error (unlikely for rdev::listen, as it's blocking), break the loop
                    break;
                }
            }
        });

        let mut in_break = false;
        loop {
            let activity_detected = self.detect_activity();

            if activity_detected {
                if in_break {
                    println!("Break End");
                    self.insert_break_end()?;
                    in_break = false;
                }
                // Ensure last_activity is updated to prevent immediate re-triggering of break
                *self.last_activity.lock().unwrap() = Instant::now();
            } else if !in_break && self.last_activity.lock().unwrap().elapsed() >= Duration::from_secs(self.config.break_threshold) {
                println!("Break Start");
                self.insert_break_start()?;
                in_break = true;
            }

            time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }

    // Checks if user activity was detected recently.
    //
    // Returns true if the last activity occurred within the configured activity_threshold.
    fn detect_activity(&self) -> bool {
        let is_active = self.last_activity.lock().unwrap().elapsed() < Duration::from_secs(self.config.activity_threshold);
        if is_active {
            println!("Activity!");
        }
        is_active
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
