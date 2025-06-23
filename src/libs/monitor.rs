use crate::db::breaks::Breaks;
use crate::db::workdays::Workdays;
use crate::libs::config::MonitorConfig;
use chrono::Local;
use rdev::{listen, EventType};
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration, Instant};

// Represents the activity monitor.
pub struct Monitor {
    pub config: MonitorConfig,
    pub breaks: Breaks,
    pub workdays: Workdays,
    pub last_activity: Arc<Mutex<Instant>>,
    pub activity_start: Arc<Mutex<Option<Instant>>>,
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
        let workdays = Workdays::new()?;
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let activity_start = Arc::new(Mutex::new(None));
        let last_activity_clone = Arc::clone(&last_activity);
        let activity_start_clone = Arc::clone(&activity_start);

        // Start a thread to listen for input events using rdev
        std::thread::spawn(move || {
            if let Err(e) = listen(move |event| match event.event_type {
                EventType::KeyPress(_)
                | EventType::KeyRelease(_)
                | EventType::ButtonPress(_)
                | EventType::ButtonRelease(_)
                | EventType::MouseMove { .. }
                | EventType::Wheel { .. } => {
                    let mut last_activity = last_activity_clone.lock().unwrap();
                    let mut activity_start = activity_start_clone.lock().unwrap();
                    *last_activity = Instant::now();
                    if activity_start.is_none() {
                        *activity_start = Some(Instant::now());
                    }
                }
            }) {
                eprintln!("Error in rdev listener: {:?}", e);
            }
        });

        Ok(Monitor {
            config,
            breaks,
            workdays,
            last_activity,
            activity_start,
        })
    }

    // Runs the main activity monitoring loop.
    //
    // Continuously checks for user activity and records breaks based on the configured
    // break_threshold and poll_interval.
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        println!(
            "Monitor is running with break threshold {}s, poll interval {}ms, activity threshold {}s",
            self.config.break_threshold, self.config.poll_interval, self.config.activity_threshold
        );
        if self.config.break_threshold == 0 {
            return Ok(());
        }

        let mut in_break = false;

        loop {
            let activity_detected = self.detect_activity();
            let today = Local::now().date_naive();

            if activity_detected {
                if in_break {
                    self.insert_break_end()?;
                    in_break = false;
                }

                // Check activity duration for workday start
                let activity_duration = self.activity_start.lock().unwrap();
                if let Some(start) = *activity_duration {
                    if start.elapsed() >= Duration::from_secs(self.config.activity_threshold) {
                        if self.workdays.fetch(today)?.is_none() {
                            println!("Starting workday for {}", today);
                            self.workdays.insert_start(today)?;
                        }
                        *self.activity_start.lock().unwrap() = None; // Reset after recording
                    }
                }
            } else if !in_break && self.last_activity.lock().unwrap().elapsed() >= Duration::from_secs(self.config.break_threshold) {
                self.insert_break_start()?;
                in_break = true;
                *self.activity_start.lock().unwrap() = None; // Reset activity start on break
            }

            time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }

    // Checks if user activity has occurred since the last poll.
    //
    // Returns true if activity was detected within the poll_interval.
    pub fn detect_activity(&self) -> bool {
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
