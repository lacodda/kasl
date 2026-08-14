//! The activity monitor: input events in, workdays and pauses out.
//!
//! A background thread listens to raw input via `rdev` and stamps
//! `last_activity`; the async loop polls that timestamp and drives a
//! two-state machine (Active / InPause) that writes workday and pause
//! records.
//!
//! ```rust,no_run
//! # async fn f() -> anyhow::Result<()> {
//! use kasl::libs::config::MonitorConfig;
//! use kasl::libs::monitor::Monitor;
//!
//! let config = MonitorConfig {
//!     pause_threshold: 120,
//!     activity_threshold: 60,
//!     poll_interval: 1000,
//!     min_pause_duration: 30,
//!     min_work_interval: 15,
//!     ..Default::default()
//! };
//!
//! let mut monitor = Monitor::new(config)?;
//! monitor.run().await?;
//! # Ok(())
//! # }
//! ```

use crate::db::pauses::Pauses;
use crate::db::workdays::Workdays;
use crate::libs::config::MonitorConfig;
use crate::libs::messages::Message;
use crate::{msg_debug, msg_error, msg_info};
use anyhow::Result;
use chrono::{Local, NaiveDate};
use rdev::{EventType, listen};
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration, Instant};
use tracing::{Level, debug, instrument, span};

/// The two states the loop moves between.
#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// Input seen recently; workday end keeps advancing.
    Active,

    /// Inactivity crossed the threshold; waiting for input to close the pause.
    InPause,
}

/// The monitor's moving parts: config, database handles, and the shared
/// timestamps the input thread writes.
pub struct Monitor {
    /// Thresholds and intervals; changes require a restart to apply.
    pub config: MonitorConfig,

    /// Pause table handle.
    pub pauses: Pauses,

    /// Workday table handle.
    pub workdays: Workdays,

    /// When input was last seen; written by the listener thread.
    pub last_activity: Arc<Mutex<Instant>>,

    /// Start of the current sustained-activity streak, or `None`.
    ///
    /// Set on the first input after quiet, cleared when a pause begins or a
    /// workday is created. Requiring the streak to outlast
    /// `activity_threshold` keeps a stray mouse nudge from starting a
    /// workday.
    pub activity_start: Arc<Mutex<Option<Instant>>>,

    /// Current loop state.
    state: State,
}

impl Monitor {
    /// Opens the database handles and spawns the input listener thread.
    ///
    /// The listener updates `last_activity` on every keyboard/mouse event
    /// and sets `activity_start` when a streak begins. Listener errors are
    /// logged, not fatal - the loop keeps running without input data rather
    /// than dying silently in the background.
    ///
    /// ```rust,no_run
    /// # async fn f() -> anyhow::Result<()> {
    /// use kasl::libs::config::MonitorConfig;
    /// use kasl::libs::monitor::Monitor;
    ///
    /// let config = MonitorConfig::default();
    /// let mut monitor = Monitor::new(config)?;
    /// monitor.run().await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(config))]
    pub fn new(config: MonitorConfig) -> Result<Self> {
        let span = span!(Level::INFO, "monitor_init");
        let _enter = span.enter();

        debug!("Initializing monitor with config: {:?}", config);

        let pauses = Pauses::new()?;
        let workdays = Workdays::new()?;

        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let activity_start = Arc::new(Mutex::new(None));

        let last_activity_clone = Arc::clone(&last_activity);
        let activity_start_clone = Arc::clone(&activity_start);

        // The listener blocks its thread, so it gets its own.
        std::thread::spawn(move || {
            if let Err(e) = listen(move |event| match event.event_type {
                EventType::KeyPress(_)
                | EventType::KeyRelease(_)
                | EventType::ButtonPress(_)
                | EventType::ButtonRelease(_)
                | EventType::MouseMove { .. }
                | EventType::Wheel { .. } => {
                    {
                        let mut last_activity = last_activity_clone.lock().unwrap();
                        *last_activity = Instant::now();
                    }

                    {
                        let mut activity_start = activity_start_clone.lock().unwrap();
                        // First input after quiet starts the sustained-activity streak.
                        if activity_start.is_none() {
                            *activity_start = Some(Instant::now());
                        }
                    }
                }
            }) {
                msg_error!(Message::ErrorInRdevListener(format!("{:?}", e)));
            }
        });

        Ok(Monitor {
            config,
            pauses,
            workdays,
            last_activity,
            activity_start,
            state: State::Active,
        })
    }

    /// Runs the polling loop until the process is stopped.
    ///
    /// Each tick checks for recent input and dispatches on (state, activity):
    /// Active+quiet may open a pause, InPause+input closes it, Active+input
    /// keeps the workday going. Database errors inside a tick are logged and
    /// the loop continues - a transient lock must not kill the daemon.
    ///
    /// `pause_threshold == 0` disables the loop entirely (returns at once).
    ///
    /// ```rust,no_run
    /// # async fn f() -> anyhow::Result<()> {
    /// use kasl::libs::config::MonitorConfig;
    /// use kasl::libs::monitor::Monitor;
    ///
    /// let config = MonitorConfig {
    ///     poll_interval: 1000,      // Check every second
    ///     pause_threshold: 120,     // Pause after 2 minutes
    ///     activity_threshold: 30,   // Workday starts after 30s
    ///     ..Default::default()
    /// };
    ///
    /// let mut monitor = Monitor::new(config)?;
    /// monitor.run().await?; // Runs indefinitely
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub async fn run(&mut self) -> Result<()> {
        msg_info!(Message::MonitorStarted {
            pause_threshold: self.config.pause_threshold,
            poll_interval: self.config.poll_interval,
            activity_threshold: self.config.activity_threshold,
        });

        // pause_threshold 0 means "no pause tracking".
        if self.config.pause_threshold == 0 {
            return Ok(());
        }

        loop {
            let activity_detected = self.detect_activity();
            let today = Local::now().date_naive();

            match self.state {
                State::Active if !activity_detected => {
                    if let Err(e) = self.handle_inactivity() {
                        msg_error!(Message::DatabaseOperationFailed {
                            operation: "handle_inactivity".to_string(),
                            error: e.to_string()
                        });
                    }
                }
                State::InPause if activity_detected => {
                    if let Err(e) = self.handle_return_from_pause() {
                        msg_error!(Message::DatabaseOperationFailed {
                            operation: "handle_return_from_pause".to_string(),
                            error: e.to_string()
                        });
                    }
                }
                State::Active if activity_detected => {
                    if let Err(e) = self.ensure_workday_started(today) {
                        msg_error!(Message::DatabaseOperationFailed {
                            operation: "ensure_workday_started".to_string(),
                            error: e.to_string()
                        });
                    }
                }
                // InPause with no activity: nothing to do.
                _ => {}
            }

            time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }

    /// True when input was seen within the last poll interval.
    ///
    /// ```rust,no_run
    /// use kasl::libs::monitor::Monitor;
    /// use kasl::libs::config::MonitorConfig;
    ///
    /// let monitor = Monitor::new(MonitorConfig::default())?;
    ///
    /// if monitor.detect_activity() {
    ///     println!("User is active");
    /// } else {
    ///     println!("User appears inactive");
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn detect_activity(&self) -> bool {
        let elapsed = self.last_activity.lock().unwrap().elapsed();
        let is_active = elapsed < Duration::from_millis(self.config.poll_interval);

        msg_debug!(format!(
            "Activity check: elapsed={:?}, active={}, threshold={:?}",
            elapsed,
            is_active,
            Duration::from_millis(self.config.poll_interval)
        ));

        is_active
    }

    /// Opens a pause once inactivity exceeds `pause_threshold`.
    ///
    /// The pause start is backdated by the threshold: the user stopped
    /// working when the input stopped, not when the detector noticed.
    /// Nothing is recorded before the workday exists - pre-work idle must
    /// not become a pause ending seconds before the day starts.
    ///
    /// ```rust,no_run
    /// use kasl::libs::config::MonitorConfig;
    ///
    /// // Called automatically by the monitoring loop; sensitivity comes
    /// // from the config:
    /// let config = MonitorConfig {
    ///     pause_threshold: 30,  // Detect pauses after 30 seconds
    ///     ..Default::default()
    /// };
    /// ```
    fn handle_inactivity(&mut self) -> Result<()> {
        let idle_time = self.last_activity.lock().unwrap().elapsed();

        if idle_time >= Duration::from_secs(self.config.pause_threshold) {
            let today = Local::now().date_naive();
            // Do not record pauses before the workday has started — otherwise
            // pre-work idle creates a pause that ends seconds before workdays.start.
            if self.workdays.fetch(today)?.is_none() {
                return Ok(());
            }

            msg_info!(Message::PauseStarted);

            // Backdate the start: the pause began when input stopped.
            let pause_start_time = Local::now().naive_local() - chrono::Duration::seconds(self.config.pause_threshold as i64);
            self.pauses.insert_start_with_time(pause_start_time)?;

            self.state = State::InPause;

            // Reset the streak so a post-pause workday start requires
            // sustained activity again.
            *self.activity_start.lock().unwrap() = None;
        }

        Ok(())
    }

    /// Closes the open pause and returns to Active.
    ///
    /// ```rust,no_run
    /// // Called automatically by the monitoring loop when input resumes;
    /// // completes the record opened by handle_inactivity.
    /// ```
    fn handle_return_from_pause(&mut self) -> Result<()> {
        msg_info!(Message::PauseEnded);
        self.pauses.insert_end()?;
        self.state = State::Active;
        Ok(())
    }

    /// Creates today's workday once sustained activity outlasts
    /// `activity_threshold`; the streak tracker is cleared afterwards so
    /// only one workday per date is created.
    ///
    /// ```rust,no_run
    /// // Called automatically during the monitoring loop. With
    /// // activity_threshold = 30: first input sets the streak start,
    /// // and 30s of continued input creates the workday.
    /// ```
    pub fn ensure_workday_started(&mut self, today: NaiveDate) -> Result<()> {
        let activity_start_time = {
            let activity_start_guard = self.activity_start.lock().unwrap();
            *activity_start_guard
        };

        if let Some(start_time) = activity_start_time {
            let activity_duration = start_time.elapsed();

            if activity_duration >= Duration::from_secs(self.config.activity_threshold) && self.workdays.fetch(today)?.is_none() {
                match self.workdays.insert_start(today) {
                    Ok(()) => {
                        msg_info!(Message::WorkdayStarting(today.to_string()));
                        *self.activity_start.lock().unwrap() = None;
                    }
                    Err(e) => {
                        // Log and keep monitoring; one failed insert must
                        // not stop the daemon.
                        msg_error!(Message::WorkdayCreateFailed);
                        debug!("Workday creation error: {:?}", e);
                    }
                }
            }
        }

        Ok(())
    }
}
