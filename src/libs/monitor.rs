//! Core activity monitoring system for kasl.
//!
//! This module implements the heart of kasl's activity tracking functionality,
//! providing real-time monitoring of user input to automatically detect work
//! sessions, breaks, and productivity patterns. It integrates keyboard and mouse
//! monitoring with intelligent state management and database recording.
//!
//! ## Architecture Overview
//!
//! The monitor system consists of several key components:
//! - **Input Detection**: Low-level keyboard and mouse event capture
//! - **State Machine**: Activity state tracking (Active/InPause)
//! - **Timing Logic**: Configurable thresholds for activity and pause detection
//! - **Database Integration**: Automatic workday and pause recording
//! - **Configuration Management**: Flexible behavior customization
//!
//! ## Activity Detection
//!
//! The system monitors these input events:
//! - **Keyboard**: Key presses and releases for typing activity
//! - **Mouse**: Button clicks, movement, and scroll wheel events
//! - **Timing**: Precise timestamp tracking for activity intervals
//!
//! ## State Management
//!
//! The monitor uses a state machine with two primary states:
//! - **Active**: User is present and working
//! - **InPause**: User is away or inactive
//!
//! State transitions are governed by configurable thresholds:
//! - `pause_threshold`: Inactivity duration before entering pause state
//! - `activity_threshold`: Activity duration required for workday start
//! - `poll_interval`: Frequency of state checks
//!
//! ## Workday Logic
//!
//! The system automatically manages workday records:
//! - **Start Detection**: Sustained activity triggers workday creation
//! - **End Detection**: Final activity determines workday end time
//! - **Pause Recording**: Significant breaks are tracked with precise timing
//! - **Data Quality**: Short intervals are filtered to improve report accuracy
//!
//! ## Configuration Examples
//!
//! ```rust
//! use kasl::libs::config::MonitorConfig;
//! use kasl::libs::monitor::Monitor;
//!
//! // Conservative configuration for busy environments
//! let config = MonitorConfig {
//!     pause_threshold: 120,     // 2 minutes before pause
//!     activity_threshold: 60,   // 1 minute to start workday
//!     poll_interval: 1000,      // Check every second
//!     min_pause_duration: 30,   // Record pauses >= 30 minutes
//!     min_work_interval: 15,    // Merge intervals < 15 minutes
//! };
//!
//! // Sensitive configuration for focused work
//! let sensitive_config = MonitorConfig {
//!     pause_threshold: 30,      // 30 seconds before pause
//!     activity_threshold: 10,   // 10 seconds to start workday
//!     poll_interval: 500,       // Check twice per second
//!     min_pause_duration: 5,    // Record pauses >= 5 minutes
//!     min_work_interval: 5,     // Merge intervals < 5 minutes
//! };
//! ```
//!
//! ## Database Integration
//!
//! The monitor automatically maintains database records:
//! - **Workdays**: Daily work session boundaries
//! - **Pauses**: Break periods with start and end times
//! - **Timestamps**: Precise timing for accurate reporting
//!
//! ## Performance Characteristics
//!
//! The monitor is designed for continuous operation with minimal resource usage:
//! - **CPU Usage**: Configurable polling interval balances responsiveness and efficiency
//! - **Memory**: Minimal state maintenance, no activity history buffering
//! - **I/O**: Batched database operations for efficiency
//! - **Network**: No network activity during monitoring

use crate::db::pauses::Pauses;
use crate::db::workdays::Workdays;
use crate::libs::config::MonitorConfig;
use crate::libs::messages::Message;
use crate::{msg_debug, msg_error, msg_info};
use anyhow::Result;
use chrono::{Local, NaiveDate};
use rdev::{listen, EventType};
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration, Instant};
use tracing::{debug, instrument, span, Level};

/// Represents the current state of the user's activity.
///
/// This enum provides a clean, explicit way to manage the monitor's operational
/// state. Using an enum instead of boolean flags makes the code more readable
/// and reduces the likelihood of state-related bugs.
///
/// ## State Transitions
///
/// ```text
/// Active ←→ InPause
/// ```
///
/// State changes are triggered by:
/// - **Active → InPause**: Inactivity exceeds `pause_threshold`
/// - **InPause → Active**: New input activity detected
///
/// ## Thread Safety
///
/// This enum implements `Copy` and `Clone` for efficient sharing across
/// threads and async tasks. The simple enum structure ensures atomic
/// operations when used with appropriate synchronization primitives.
#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// The user is currently active and not on a pause.
    ///
    /// In this state, the monitor:
    /// - Continues tracking input activity
    /// - Updates workday end times with each activity
    /// - Monitors for inactivity to detect pause start
    /// - Ensures workday creation if sustained activity is detected
    Active,

    /// The user is currently on a pause due to inactivity.
    ///
    /// In this state, the monitor:
    /// - Waits for activity to resume
    /// - Does not update workday end times
    /// - Prepares to record pause end time when activity resumes
    /// - Resets workday start detection when returning to active
    InPause,
}

/// The core activity monitor responsible for tracking user presence
/// and managing workday and pause records.
///
/// This struct orchestrates all aspects of activity monitoring, from low-level
/// input detection to high-level workday management. It maintains the necessary
/// state and provides the main monitoring loop that runs continuously during
/// active monitoring sessions.
///
/// ## Component Architecture
///
/// ```text
/// ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
/// │   Input Events  │───▶│     Monitor     │───▶│    Database     │
/// │ (keyboard/mouse)│    │  (state logic)  │    │ (workdays/pauses)│
/// └─────────────────┘    └─────────────────┘    └─────────────────┘
/// ```
///
/// ## Thread Safety
///
/// The monitor uses thread-safe primitives to coordinate between:
/// - **Input Thread**: Captures keyboard/mouse events via `rdev`
/// - **Monitor Thread**: Runs the main monitoring loop
/// - **Shared State**: Activity timestamps and workday tracking
///
/// ## Configuration Impact
///
/// All timing behavior is controlled by the [`MonitorConfig`]:
/// - **Responsiveness**: Lower `poll_interval` = more responsive state changes
/// - **Sensitivity**: Lower `pause_threshold` = more sensitive pause detection
/// - **Workday Logic**: Higher `activity_threshold` = more deliberate workday starts
/// - **Data Quality**: Higher `min_*` values = cleaner, less noisy data
pub struct Monitor {
    /// Configuration settings for the monitor, such as thresholds.
    ///
    /// This configuration controls all timing and behavior aspects of the
    /// monitor. Changes to this configuration require restarting the monitor
    /// to take effect, as the values are used throughout the monitoring loop.
    pub config: MonitorConfig,

    /// Database interface for managing pause records.
    ///
    /// This interface handles all pause-related database operations:
    /// - Recording pause start times when inactivity is detected
    /// - Recording pause end times when activity resumes
    /// - Querying existing pause data for validation and reporting
    pub pauses: Pauses,

    /// Database interface for managing workday records.
    ///
    /// This interface handles workday lifecycle management:
    /// - Creating new workday records when sustained activity is detected
    /// - Updating workday end times as activity continues
    /// - Querying workday status for duplicate prevention
    pub workdays: Workdays,

    /// Timestamp of the last detected user activity (keyboard, mouse).
    ///
    /// This timestamp is continuously updated by the input event listener
    /// running in a separate thread. It's protected by a Mutex for thread-safe
    /// access between the input thread and the monitoring loop.
    ///
    /// The timestamp is used to:
    /// - Calculate inactivity duration for pause detection
    /// - Determine if activity is "recent" for state transitions
    /// - Provide timing information for workday management
    pub last_activity: Arc<Mutex<Instant>>,

    /// Optional timestamp marking the beginning of a period of sustained activity.
    ///
    /// This field implements a "sustained activity" detection mechanism to prevent
    /// false workday starts from brief, accidental input events. The timestamp is:
    /// - Set when activity begins after a period of inactivity
    /// - Reset to None when a pause begins or workday is created
    /// - Used to calculate activity duration for workday start logic
    ///
    /// ## Workday Start Logic
    ///
    /// A workday is created when:
    /// 1. `activity_start` is set (continuous activity period began)
    /// 2. Duration since `activity_start` exceeds `activity_threshold`
    /// 3. No workday record exists for the current date
    pub activity_start: Arc<Mutex<Option<Instant>>>,

    /// The current operational state of the monitor (Active or InPause).
    ///
    /// This field tracks the monitor's current state and drives the main
    /// monitoring loop logic. State transitions trigger various actions:
    /// - Active → InPause: Record pause start, reset activity tracking
    /// - InPause → Active: Record pause end, resume workday tracking
    state: State,
}

impl Monitor {
    /// Creates a new `Monitor` instance.
    ///
    /// This constructor initializes all monitor components and sets up the
    /// background input event listener. It performs several critical setup
    /// operations that are essential for proper monitoring functionality.
    ///
    /// ## Initialization Process
    ///
    /// 1. **Database Connections**: Establishes connections to workday and pause databases
    /// 2. **Shared State**: Creates thread-safe containers for activity tracking
    /// 3. **Input Listener**: Spawns background thread for keyboard/mouse monitoring
    /// 4. **State Setup**: Initializes monitor in Active state
    ///
    /// ## Input Event Handling
    ///
    /// The constructor spawns a dedicated thread running `rdev::listen()` to capture:
    /// - **Keyboard Events**: Key presses and releases
    /// - **Mouse Events**: Button clicks, movements, and scroll wheel
    /// - **Timestamp Updates**: Continuous activity timestamp maintenance
    ///
    /// ## Thread Architecture
    ///
    /// ```text
    /// Main Thread          Input Thread
    /// ┌─────────────┐     ┌─────────────────┐
    /// │   Monitor   │────▶│  rdev::listen() │
    /// │    Loop     │     │   (keyboard/    │
    /// │             │◀────│     mouse)      │
    /// └─────────────┘     └─────────────────┘
    ///        │                      │
    ///        └──── Shared State ────┘
    ///        (last_activity, activity_start)
    /// ```
    ///
    /// ## Error Handling
    ///
    /// The input listener includes error handling for:
    /// - Input device access failures
    /// - Platform-specific event capture issues
    /// - Thread communication problems
    ///
    /// Errors in the input thread are logged but don't crash the main monitor,
    /// allowing for graceful degradation when input monitoring isn't available.
    ///
    /// # Arguments
    ///
    /// * `config` - The [`MonitorConfig`] containing timing and behavior settings
    ///
    /// # Returns
    ///
    /// Returns `Ok(Monitor)` with a fully initialized monitor ready to start
    /// the monitoring loop, or an error if database initialization fails.
    ///
    /// # Error Scenarios
    ///
    /// - **Database Connection**: Cannot connect to SQLite database files
    /// - **Database Schema**: Database schema is incompatible or corrupted
    /// - **File Permissions**: Cannot read/write database files
    /// - **Resource Exhaustion**: System cannot create necessary threads or allocate memory
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::config::MonitorConfig;
    /// use kasl::libs::monitor::Monitor;
    ///
    /// // Create monitor with default configuration
    /// let config = MonitorConfig::default();
    /// let monitor = Monitor::new(config)?;
    ///
    /// // Start monitoring
    /// monitor.run().await?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[instrument(skip(config))]
    pub fn new(config: MonitorConfig) -> Result<Self> {
        let span = span!(Level::INFO, "monitor_init");
        let _enter = span.enter();

        debug!("Initializing monitor with config: {:?}", config);

        // Initialize database connections
        // These connections will be used throughout the monitor's lifetime
        let pauses = Pauses::new()?;
        let workdays = Workdays::new()?;

        // Create shared state containers for cross-thread communication
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let activity_start = Arc::new(Mutex::new(None));

        // Clone Arc references for the input event listener thread
        // This allows the background thread to update shared state
        let last_activity_clone = Arc::clone(&last_activity);
        let activity_start_clone = Arc::clone(&activity_start);

        // Spawn a new thread to listen for device events
        // This ensures the main monitor loop is not blocked by event listening
        std::thread::spawn(move || {
            if let Err(e) = listen(move |event| match event.event_type {
                // Monitor all types of user input for activity detection
                EventType::KeyPress(_)
                | EventType::KeyRelease(_)
                | EventType::ButtonPress(_)
                | EventType::ButtonRelease(_)
                | EventType::MouseMove { .. }
                | EventType::Wheel { .. } => {
                    // Update activity tracking with current timestamp
                    {
                        let mut last_activity = last_activity_clone.lock().unwrap();
                        *last_activity = Instant::now();
                    }

                    // Manage sustained activity tracking for workday detection
                    {
                        let mut activity_start = activity_start_clone.lock().unwrap();

                        // If this is the first activity after inactivity, mark the start
                        // This begins the sustained activity period for workday detection
                        if activity_start.is_none() {
                            *activity_start = Some(Instant::now());
                        }
                    }
                }
            }) {
                // Log input listener errors but don't crash the application
                // This allows the monitor to continue functioning even if input
                // monitoring encounters platform-specific issues
                msg_error!(Message::ErrorInRdevListener(format!("{:?}", e)));
            }
        });

        Ok(Monitor {
            config,
            pauses,
            workdays,
            last_activity,
            activity_start,
            state: State::Active, // Initialize the monitor in the Active state
        })
    }

    /// Runs the main monitoring loop.
    ///
    /// This asynchronous function implements the core monitoring logic that runs
    /// continuously until the monitor is stopped. It orchestrates state management,
    /// activity detection, and database operations in a coordinated fashion.
    ///
    /// ## Loop Architecture
    ///
    /// The monitoring loop operates on a fixed polling interval and performs
    /// these operations each cycle:
    ///
    /// 1. **Activity Detection**: Check if recent input activity occurred
    /// 2. **State Evaluation**: Determine appropriate state based on activity
    /// 3. **State Transitions**: Handle transitions between Active and InPause
    /// 4. **Workday Management**: Ensure workday records are properly maintained
    /// 5. **Sleep**: Wait for the next polling interval
    ///
    /// ## State Machine Logic
    ///
    /// ```text
    /// ┌─────────────────────────────────────────────────────────────┐
    /// │                    Monitor Loop                             │
    /// │                                                             │
    /// │  ┌─────────────┐    Activity?    ┌─────────────────────┐   │
    /// │  │   Active    │─────────No─────▶│  handle_inactivity  │   │
    /// │  │             │                 │                     │   │
    /// │  │             │◀────────────────│  (start pause)      │   │
    /// │  └─────────────┘                 └─────────────────────┘   │
    /// │        │                                                   │
    /// │     Activity                                                │
    /// │        │                                                   │
    /// │        ▼                                                   │
    /// │  ┌─────────────────────┐                                   │
    /// │  │ ensure_workday_     │                                   │
    /// │  │ started             │                                   │
    /// │  └─────────────────────┘                                   │
    /// │                                                             │
    /// │  ┌─────────────┐    Activity?    ┌─────────────────────┐   │
    /// │  │  InPause    │─────────Yes────▶│handle_return_from_  │   │
    /// │  │             │                 │pause                │   │
    /// │  │             │◀────────────────│                     │   │
    /// │  └─────────────┘                 │  (end pause)        │   │
    /// │                                  └─────────────────────┘   │
    /// └─────────────────────────────────────────────────────────────┘
    /// ```
    ///
    /// ## Configuration-Driven Behavior
    ///
    /// The loop behavior is entirely controlled by the [`MonitorConfig`]:
    /// - **`poll_interval`**: Controls loop frequency and CPU usage
    /// - **`pause_threshold`**: Determines when inactivity becomes a pause
    /// - **`activity_threshold`**: Controls workday start detection sensitivity
    /// - **Special Case**: If `pause_threshold` is 0, monitoring is disabled
    ///
    /// ## Database Operations
    ///
    /// The loop performs these database operations as needed:
    /// - **Workday Creation**: When sustained activity is detected
    /// - **Pause Recording**: When inactivity exceeds threshold
    /// - **Pause Completion**: When activity resumes after pause
    /// - **Workday Updates**: Continuous end time updates during activity
    ///
    /// ## Error Handling
    ///
    /// Database errors during the monitoring loop are handled gracefully:
    /// - Errors are logged with detailed information
    /// - The loop continues running to avoid service interruption
    /// - Critical errors are propagated to stop the monitor
    ///
    /// ## Performance Characteristics
    ///
    /// - **CPU Usage**: Directly proportional to polling frequency (1/poll_interval)
    /// - **Memory Usage**: Constant, no accumulation of historical data
    /// - **I/O Operations**: Minimal, only database writes for state changes
    /// - **Network Usage**: None during monitoring
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when the monitoring loop is explicitly stopped,
    /// or an error if a critical database operation fails that prevents
    /// continued monitoring.
    ///
    /// # Error Scenarios
    ///
    /// - **Database Connection Loss**: SQLite database becomes unavailable
    /// - **Disk Space Exhaustion**: Cannot write to database files
    /// - **Permission Changes**: Database files become read-only
    /// - **System Resource Exhaustion**: Cannot allocate memory for operations
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::config::MonitorConfig;
    /// use kasl::libs::monitor::Monitor;
    ///
    /// // Start monitoring with custom configuration
    /// let config = MonitorConfig {
    ///     poll_interval: 1000,      // Check every second
    ///     pause_threshold: 120,     // Pause after 2 minutes
    ///     activity_threshold: 30,   // Workday starts after 30s
    ///     ..Default::default()
    /// };
    ///
    /// let mut monitor = Monitor::new(config)?;
    /// monitor.run().await?; // Runs indefinitely
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    #[instrument(skip(self))]
    pub async fn run(&mut self) -> Result<()> {
        msg_info!(Message::MonitorStarted {
            pause_threshold: self.config.pause_threshold,
            poll_interval: self.config.poll_interval,
            activity_threshold: self.config.activity_threshold,
        });

        // Special case: if pause threshold is 0, pauses are disabled
        // This allows users to disable pause tracking while keeping workday tracking
        if self.config.pause_threshold == 0 {
            return Ok(());
        }

        // The main loop that periodically checks for activity and updates state
        loop {
            let activity_detected = self.detect_activity();
            let today = Local::now().date_naive();

            // State machine logic - handle state transitions based on current state and activity
            match self.state {
                // Currently active, but no recent activity detected
                State::Active if !activity_detected => {
                    self.handle_inactivity()?;
                }
                // Currently in pause, but activity has resumed
                State::InPause if activity_detected => {
                    self.handle_return_from_pause()?;
                }
                // Currently active with ongoing activity - ensure workday is tracked
                State::Active if activity_detected => {
                    self.ensure_workday_started(today)?;
                }
                // No action needed for: InPause with no activity
                _ => {}
            }

            // Wait for the configured poll interval before the next check
            // This controls the monitoring loop frequency and CPU usage
            time::sleep(Duration::from_millis(self.config.poll_interval)).await;
        }
    }

    /// Checks if any user activity has been detected within the last poll interval.
    ///
    /// This method implements the core activity detection logic that drives
    /// the monitor's state machine. It determines whether the user is currently
    /// active based on the recency of input events captured by the background
    /// input listener thread.
    ///
    /// ## Detection Logic
    ///
    /// Activity is considered "detected" when the time elapsed since the last
    /// input event is less than the polling interval. This approach ensures
    /// that activity detection is synchronized with the monitoring loop's
    /// polling frequency.
    ///
    /// ```text
    /// Timeline: ───●───●─────────●──────────────────▶
    ///          input input   poll               now
    ///                          ↑
    ///                   elapsed < poll_interval
    ///                     = Activity Detected
    /// ```
    ///
    /// ## Sensitivity Tuning
    ///
    /// The detection sensitivity can be adjusted through configuration:
    /// - **Lower `poll_interval`**: More sensitive, detects brief activity
    /// - **Higher `poll_interval`**: Less sensitive, requires sustained activity
    ///
    /// ## Thread Safety
    ///
    /// This method safely accesses the shared `last_activity` timestamp
    /// that is continuously updated by the input listener thread. The mutex
    /// protection ensures data consistency without blocking the input thread.
    ///
    /// ## Debug Logging
    ///
    /// When debug mode is enabled (`KASL_DEBUG=1`), this method logs detailed
    /// timing information to help with troubleshooting and configuration tuning:
    /// - Elapsed time since last activity
    /// - Activity detection result
    /// - Timing relationship to poll interval
    ///
    /// # Returns
    ///
    /// Returns `true` if user activity was detected within the last poll interval,
    /// `false` if the user appears to be inactive.
    ///
    /// # Performance Considerations
    ///
    /// This method is called once per polling cycle and performs minimal work:
    /// - Single mutex lock/unlock operation
    /// - Simple timestamp arithmetic
    /// - Optional debug logging
    ///
    /// The implementation is designed for frequent calling with minimal overhead.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::monitor::Monitor;
    /// use kasl::libs::config::MonitorConfig;
    ///
    /// let monitor = Monitor::new(MonitorConfig::default())?;
    ///
    /// // Check for activity
    /// if monitor.detect_activity() {
    ///     println!("User is active");
    /// } else {
    ///     println!("User appears inactive");
    /// }
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn detect_activity(&self) -> bool {
        let elapsed = self.last_activity.lock().unwrap().elapsed();

        // Activity is considered detected if the time since last_activity
        // is less than the poll_interval. This creates a sliding window
        // of activity detection that aligns with the monitoring loop timing.
        let is_active = elapsed < Duration::from_millis(self.config.poll_interval);

        // Debug logging only visible with KASL_DEBUG=1
        // This helps with configuration tuning and troubleshooting
        msg_debug!(format!(
            "Activity check: elapsed={:?}, active={}, threshold={:?}",
            elapsed,
            is_active,
            Duration::from_millis(self.config.poll_interval)
        ));

        is_active
    }

    /// Handles the scenario when user inactivity is detected.
    ///
    /// This method implements the transition from Active to InPause state when
    /// the user has been inactive for longer than the configured pause threshold.
    /// It manages both the state transition and the database recording of the
    /// pause event.
    ///
    /// ## Inactivity Assessment
    ///
    /// The method calculates total inactivity duration by examining the time
    /// elapsed since the last recorded input activity. If this duration exceeds
    /// the configured `pause_threshold`, a pause is initiated.
    ///
    /// ## Pause Recording Strategy
    ///
    /// When recording a pause, the system uses retroactive timing to ensure
    /// accuracy:
    /// ```text
    /// Timeline: ──●─────────────────●──────▶
    ///          last              threshold
    ///         activity            exceeded
    ///             ↑                   ↑
    ///      pause actually         pause
    ///         started            detected
    /// ```
    ///
    /// The pause start time is calculated as:
    /// `current_time - pause_threshold`
    ///
    /// This approach ensures that the recorded pause start time reflects when
    /// the user actually became inactive, not when the system detected it.
    ///
    /// ## State Management
    ///
    /// When transitioning to pause state, the method performs several actions:
    /// 1. **Database Recording**: Inserts pause start record with calculated timing
    /// 2. **State Transition**: Changes monitor state from Active to InPause
    /// 3. **Activity Reset**: Clears the sustained activity tracker
    ///
    /// ## Activity Tracking Reset
    ///
    /// The `activity_start` timestamp is reset to `None` during pause initiation.
    /// This is crucial for preventing incorrect workday start detection when
    /// activity resumes after a pause. It ensures that workday logic requires
    /// sustained activity rather than brief resume activity.
    ///
    /// ## User Notification
    ///
    /// The method provides user feedback about pause detection through
    /// informational messages. In foreground mode, users see real-time
    /// pause notifications for immediate feedback.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the pause was successfully recorded and state
    /// transition completed, or an error if database operations fail.
    ///
    /// # Error Scenarios
    ///
    /// - **Database Connection**: Cannot connect to pause database
    /// - **Database Write**: Cannot insert pause start record
    /// - **Timing Calculation**: System clock issues affecting timestamp calculation
    ///
    /// # Database Schema Impact
    ///
    /// This method creates records in the `pauses` table:
    /// ```sql
    /// INSERT INTO pauses (date, start_time) VALUES (?, ?);
    /// ```
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// // This method is called automatically by the monitoring loop
    /// // when inactivity exceeds the configured threshold
    ///
    /// // Example configuration for sensitive pause detection:
    /// let config = MonitorConfig {
    ///     pause_threshold: 30,  // Detect pauses after 30 seconds
    ///     ..Default::default()
    /// };
    /// ```
    fn handle_inactivity(&mut self) -> Result<()> {
        let idle_time = self.last_activity.lock().unwrap().elapsed();

        // Only initiate pause if inactivity exceeds the configured threshold
        if idle_time >= Duration::from_secs(self.config.pause_threshold) {
            msg_info!(Message::PauseStarted);

            // Calculate the actual pause start time by subtracting the threshold
            // This provides more accurate pause timing in reports
            let pause_start_time = Local::now().naive_local() - chrono::Duration::seconds(self.config.pause_threshold as i64);

            // Record the pause start in the database
            self.pauses.insert_start_with_time(pause_start_time)?;

            // Transition to pause state
            self.state = State::InPause;

            // Reset the activity_start timer when a pause begins
            // This prevents incorrect workday start detection after pause ends
            *self.activity_start.lock().unwrap() = None;
        }

        Ok(())
    }

    /// Handles the scenario when user activity resumes after a pause.
    ///
    /// This method manages the transition from InPause back to Active state
    /// when user input activity is detected after a period of inactivity.
    /// It completes the pause record in the database and prepares the monitor
    /// for resumed activity tracking.
    ///
    /// ## Pause Completion
    ///
    /// When activity resumes, the method completes the pause record by:
    /// 1. Recording the current time as the pause end time
    /// 2. Calculating the total pause duration
    /// 3. Updating the database with the complete pause record
    ///
    /// ## State Transition
    ///
    /// The transition from InPause to Active involves:
    /// - **Database Update**: Recording pause end time
    /// - **State Change**: Setting monitor state to Active
    /// - **Activity Preparation**: Resuming normal activity tracking
    ///
    /// ## Activity Tracking Resumption
    ///
    /// After returning from pause, the monitor resumes normal operation:
    /// - Input events are again tracked for workday management
    /// - The `activity_start` timer may be set for new sustained activity detection
    /// - Workday end times will be updated with continued activity
    ///
    /// ## Timing Accuracy
    ///
    /// The pause end time is recorded as the current timestamp when activity
    /// is first detected after the pause period. This provides accurate
    /// timing for pause duration calculations in reports.
    ///
    /// ## User Feedback
    ///
    /// The method provides immediate notification about pause completion,
    /// which is especially useful in foreground monitoring mode for
    /// real-time activity awareness.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the pause end was successfully recorded and the
    /// state transition completed, or an error if database operations fail.
    ///
    /// # Error Scenarios
    ///
    /// - **Database Connection**: Cannot connect to pause database
    /// - **Database Update**: Cannot record pause end time
    /// - **Inconsistent State**: No active pause record to complete
    ///
    /// # Database Schema Impact
    ///
    /// This method updates records in the `pauses` table:
    /// ```sql
    /// UPDATE pauses SET end_time = ? WHERE end_time IS NULL AND date = ?;
    /// ```
    ///
    /// # Pause Duration Calculation
    ///
    /// After this method completes, the pause duration can be calculated as:
    /// ```
    /// duration = end_time - start_time
    /// ```
    ///
    /// This duration is used in productivity reports and statistics.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// // This method is called automatically by the monitoring loop
    /// // when activity resumes after a pause period
    ///
    /// // The resulting pause record will include:
    /// // - start_time: When inactivity was first detected
    /// // - end_time: When activity resumed (current time)
    /// // - date: The date when the pause occurred
    /// ```
    fn handle_return_from_pause(&mut self) -> Result<()> {
        msg_info!(Message::PauseEnded);

        // Record the pause end time in the database
        // This completes the pause record that was started in handle_inactivity()
        self.pauses.insert_end()?;

        // Transition back to active state
        self.state = State::Active;

        Ok(())
    }

    /// Ensures that a workday record has been started for the current day
    /// if sustained activity is detected.
    ///
    /// This method implements intelligent workday start detection to automatically
    /// create workday records when the user begins sustained work activity. It
    /// prevents false workday starts from brief, accidental input while ensuring
    /// that genuine work sessions are properly captured.
    ///
    /// ## Sustained Activity Logic
    ///
    /// The workday start detection uses a multi-stage process:
    ///
    /// 1. **Activity Initiation**: `activity_start` timestamp is set by input events
    /// 2. **Duration Check**: Calculate time elapsed since activity started
    /// 3. **Threshold Validation**: Compare duration against `activity_threshold`
    /// 4. **Workday Creation**: Create workday record if threshold is exceeded
    /// 5. **Tracker Reset**: Clear `activity_start` to prevent duplicate creation
    ///
    /// ```text
    /// Timeline: ──●───●───●───●──────────●─────▶
    ///          start              threshold   now
    ///         activity             exceeded
    ///             ↑                   ↑
    ///      activity_start        workday
    ///         is set             created
    /// ```
    ///
    /// ## Duplicate Prevention
    ///
    /// The method includes multiple safeguards against duplicate workday creation:
    /// - **Existence Check**: Verifies no workday record exists for the date
    /// - **Activity Validation**: Requires valid `activity_start` timestamp
    /// - **Threshold Enforcement**: Demands sustained activity duration
    /// - **Reset Mechanism**: Clears tracker after successful creation
    ///
    /// ## Configuration Impact
    ///
    /// The `activity_threshold` setting controls workday start sensitivity:
    /// - **Lower Values (10-30s)**: Quick workday detection, good for focused work
    /// - **Higher Values (60-120s)**: Conservative detection, avoids false starts
    /// - **Very High Values (300s+)**: Only detects deliberate work sessions
    ///
    /// ## Database Operations
    ///
    /// When creating a workday, the method:
    /// 1. Queries for existing workday records on the target date
    /// 2. Creates a new workday record with the current timestamp
    /// 3. Handles any database errors gracefully
    ///
    /// ## Error Handling
    ///
    /// Database errors during workday creation are logged but don't crash
    /// the monitor. This ensures continuous monitoring even if individual
    /// database operations encounter issues.
    ///
    /// ## State Management
    ///
    /// After successful workday creation:
    /// - `activity_start` is reset to `None`
    /// - Monitor continues tracking for pause detection
    /// - Future activity updates the workday end time
    /// - No additional workday records are created for the date
    ///
    /// # Arguments
    ///
    /// * `today` - The current date for workday record creation
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if workday management completed successfully,
    /// or an error if critical database operations fail.
    ///
    /// # Error Scenarios
    ///
    /// - **Database Connection**: Cannot connect to workdays database
    /// - **Database Query**: Cannot check for existing workday records
    /// - **Database Insert**: Cannot create new workday record
    /// - **Date Validation**: Invalid date format or system clock issues
    ///
    /// # Database Schema Impact
    ///
    /// This method may create records in the `workdays` table:
    /// ```sql
    /// INSERT INTO workdays (date, start_time) VALUES (?, ?);
    /// ```
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// // This method is called automatically during the monitoring loop
    /// // when the user is active and no workday exists for the current date
    ///
    /// // Example: User starts work at 9:00 AM
    /// // - First input at 9:00:00 sets activity_start
    /// // - Continued input until 9:00:30
    /// // - If activity_threshold = 30s, workday is created at 9:00:30
    /// // - Workday start_time is recorded as current timestamp
    /// ```
    fn ensure_workday_started(&mut self, today: NaiveDate) -> Result<()> {
        // Check if we have an activity start timestamp
        let activity_start_time = {
            let activity_start_guard = self.activity_start.lock().unwrap();
            *activity_start_guard
        };

        // Only proceed if we have been tracking sustained activity
        if let Some(start_time) = activity_start_time {
            // Calculate how long the current activity period has lasted
            let activity_duration = start_time.elapsed();

            // Check if sustained activity exceeds the configured threshold
            if activity_duration >= Duration::from_secs(self.config.activity_threshold) {
                // Verify that no workday record already exists for today
                if self.workdays.fetch(today)?.is_none() {
                    // Create new workday record with current timestamp
                    match self.workdays.insert_start(today) {
                        Ok(()) => {
                            msg_info!(Message::WorkdayStarting(today.to_string()));

                            // Reset activity_start to prevent duplicate workday creation
                            // This ensures we only create one workday per date
                            *self.activity_start.lock().unwrap() = None;
                        }
                        Err(e) => {
                            // Log workday creation errors but continue monitoring
                            // This prevents single database errors from stopping the monitor
                            msg_error!(Message::WorkdayCreateFailed);
                            debug!("Workday creation error: {:?}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
