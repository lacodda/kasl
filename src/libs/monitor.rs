use crate::db::pauses::Pauses;
use crate::db::workdays::Workdays;
use crate::libs::config::MonitorConfig;
use crate::libs::messages::Message;
use crate::{msg_error, msg_info};
use anyhow::Result;
use chrono::{Local, NaiveDate};
use rdev::{listen, EventType};
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration, Instant};

/// Represents the current state of the user's activity.
/// Using an enum provides a more explicit and robust way to manage state
/// compared to a simple boolean flag.
#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// The user is currently active and not on a pause.
    Active,
    /// The user is currently on a pause due to inactivity.
    InPause,
}

/// The core activity monitor responsible for tracking user presence
/// and managing workday and pause records.
pub struct Monitor {
    /// Configuration settings for the monitor, such as thresholds.
    pub config: MonitorConfig,
    /// Database interface for managing pause records.
    pub pauses: Pauses,
    /// Database interface for managing workday records.
    pub workdays: Workdays,
    /// Timestamp of the last detected user activity (keyboard, mouse).
    pub last_activity: Arc<Mutex<Instant>>,
    /// Optional timestamp marking the beginning of a period of sustained activity.
    /// This is used to determine if a workday has truly started.
    pub activity_start: Arc<Mutex<Option<Instant>>>,
    /// The current operational state of the monitor (Active or InPause).
    state: State,
}

impl Monitor {
    /// Creates a new `Monitor` instance.
    ///
    /// Initializes database connections and spawns a background thread
    /// to listen for input device events (keyboard, mouse) to track activity.
    ///
    /// # Arguments
    /// * `config` - The configuration for the monitor.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if initialization fails.
    pub fn new(config: MonitorConfig) -> Result<Self> {
        let pauses = Pauses::new()?;
        let workdays = Workdays::new()?;
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let activity_start = Arc::new(Mutex::new(None));

        // Clone Arc for the separate thread to avoid ownership issues.
        let last_activity_clone = Arc::clone(&last_activity);
        let activity_start_clone = Arc::clone(&activity_start);

        // Spawn a new thread to listen for device events.
        // This ensures the main monitor loop is not blocked by event listening.
        std::thread::spawn(move || {
            if let Err(e) = listen(move |event| match event.event_type {
                EventType::KeyPress(_)
                | EventType::KeyRelease(_)
                | EventType::ButtonPress(_)
                | EventType::ButtonRelease(_)
                | EventType::MouseMove { .. }
                | EventType::Wheel { .. } => {
                    // Update `last_activity` with the current time on any detected input.
                    let mut last_activity = last_activity_clone.lock().unwrap();
                    let mut activity_start = activity_start_clone.lock().unwrap();
                    *last_activity = Instant::now();

                    // If `activity_start` is not set, set it to the current time.
                    // This marks the beginning of a continuous activity period.
                    if activity_start.is_none() {
                        *activity_start = Some(Instant::now());
                    }
                }
            }) {
                // In a production application, consider using a proper logging framework
                // like `tracing::error!` for better error handling and visibility.
                msg_error!(Message::ErrorInRdevListener(format!("{:?}", e)));
            }
        });

        Ok(Monitor {
            config,
            pauses,
            workdays,
            last_activity,
            activity_start,
            state: State::Active, // Initialize the monitor in the Active state.
        })
    }

    /// Runs the main monitoring loop.
    ///
    /// This asynchronous function continuously checks for user activity
    /// and transitions between `Active` and `InPause` states, recording
    /// pause times and ensuring workday start times are captured.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if a database operation fails.
    pub async fn run(&mut self) -> Result<()> {
        msg_info!(Message::MonitorStarted {
            pause_threshold: self.config.pause_threshold,
            poll_interval: self.config.poll_interval,
            activity_threshold: self.config.activity_threshold,
        });

        // If pause threshold is 0, pauses are disabled, so the monitor can exit.
        if self.config.pause_threshold == 0 {
            return Ok(());
        }

        // The main loop that periodically checks for activity and updates state.
        loop {
            let activity_detected = self.detect_activity();
            let today = Local::now().date_naive();

            match self.state {
                State::Active if !activity_detected => self.handle_inactivity()?,
                State::InPause if activity_detected => self.handle_return_from_pause()?,
                State::Active if activity_detected => self.ensure_workday_started(today)?,
                // No action needed if in pause and no activity, or other combinations.
                _ => {}
            }

            // Pause for the configured poll interval before the next check.
            time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }

    /// Checks if any user activity has been detected within the last poll interval.
    ///
    /// # Returns
    /// `true` if activity was detected, `false` otherwise.
    pub fn detect_activity(&self) -> bool {
        let elapsed = self.last_activity.lock().unwrap().elapsed();
        // Activity is considered detected if the time since `last_activity`
        // is less than the `poll_interval`.
        elapsed < Duration::from_millis(self.config.poll_interval)
    }

    /// Handles the scenario when user inactivity is detected.
    ///
    /// If the idle time exceeds the `pause_threshold`, a new pause is recorded,
    /// and the monitor transitions to the `InPause` state. The `activity_start`
    /// timer is also reset.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if a database operation fails.
    fn handle_inactivity(&mut self) -> Result<()> {
        let idle_time = self.last_activity.lock().unwrap().elapsed();
        if idle_time >= Duration::from_secs(self.config.pause_threshold) {
            msg_info!(Message::PauseStarted);

            // Calculate the actual pause start time by subtracting the threshold
            let pause_start_time = Local::now().naive_local() - chrono::Duration::seconds(self.config.pause_threshold as i64);
            self.pauses.insert_start_with_time(pause_start_time)?;

            self.state = State::InPause;
            // Crucially, reset the `activity_start` timer when a pause begins.
            // This prevents incorrect workday start detection after a pause ends.
            *self.activity_start.lock().unwrap() = None;
        }
        Ok(())
    }

    /// Handles the scenario when user activity resumes after a pause.
    ///
    /// Records the end of the pause and transitions the monitor back to the
    /// `Active` state.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if a database operation fails.
    fn handle_return_from_pause(&mut self) -> Result<()> {
        msg_info!(Message::PauseEnded);
        self.pauses.insert_end()?;
        self.state = State::Active;
        Ok(())
    }

    /// Ensures that a workday record has been started for the current day
    /// if sustained activity is detected.
    ///
    /// If `activity_start` is set and the duration of continuous activity
    /// exceeds `activity_threshold`, and no workday record exists for `today`,
    /// a new workday record is inserted. The `activity_start` timer is then
    /// reset to `None` to prevent re-triggering this logic for the same workday.
    ///
    /// # Arguments
    /// * `today` - The current date.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if a database operation fails.
    pub fn ensure_workday_started(&mut self, today: NaiveDate) -> Result<()> {
        let mut activity_start_lock = self.activity_start.lock().unwrap();
        if let Some(start_time) = *activity_start_lock {
            if start_time.elapsed() >= Duration::from_secs(self.config.activity_threshold) {
                // Only insert a new workday if one doesn't already exist for today.
                if self.workdays.fetch(today)?.is_none() {
                    msg_info!(Message::WorkdayStarting(today.to_string()));
                    self.workdays.insert_start(today)?;
                }
                // Reset `activity_start` after a workday is confirmed.
                // This ensures this logic doesn't repeatedly try to start the workday.
                *activity_start_lock = None;
            }
        }
        Ok(())
    }
}
