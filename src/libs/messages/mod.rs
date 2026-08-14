//! All user-facing text, one enum away from the code that prints it.
//!
//! Keeping every string behind [`Message`] means the wording lives in one
//! place, the compiler checks the parameters, and a future locale layer has a
//! single seam to hook into.
//!
//! ```rust
//! use kasl::libs::messages::{Message, success, error};
//! use kasl::{msg_info, msg_error, msg_success};
//!
//! msg_success!(Message::TaskCreated);
//! msg_error!(Message::ConfigSaveError);
//! msg_info!(Message::MonitorStarted {
//!     pause_threshold: 60,
//!     poll_interval: 500,
//!     activity_threshold: 30,
//! });
//! ```

pub mod display;
pub mod macros;
pub mod types;

pub use types::Message;

/// Prefixes the message with the success mark.
///
/// ```rust
/// use kasl::libs::messages::{Message, success};
///
/// let message = success(Message::TaskCreated);
/// println!("{}", message); // "✅ Task created successfully"
///
/// let report_message = success(Message::DailyReportSent("2025-01-15".to_string()));
/// println!("{}", report_message);
/// ```
///
/// The [`crate::msg_success`] macro prints the same thing directly:
/// ```rust
/// use kasl::msg_success;
/// use kasl::libs::messages::Message;
///
/// msg_success!(Message::TaskCreated);
/// ```
pub fn success(msg: Message) -> String {
    format!("✅ {}", msg)
}

/// Prefixes the message with the error mark.
///
/// ```rust
/// use kasl::libs::messages::{Message, error};
///
/// let message = error(Message::ConfigSaveError);
/// println!("{}", message); // "❌ Failed to save configuration"
///
/// let detailed = error(Message::GitlabFetchFailed("Network timeout".to_string()));
/// println!("{}", detailed);
/// ```
///
/// ```rust
/// use kasl::msg_error;
/// use kasl::libs::messages::Message;
/// use anyhow::Result;
///
/// # fn operation() -> Result<()> { Ok(()) }
/// fn save_config() -> Result<()> {
///     if let Err(_) = operation() {
///         msg_error!(Message::ConfigSaveError);
///         return Err(anyhow::anyhow!("Configuration save failed"));
///     }
///     Ok(())
/// }
/// ```
pub fn error(msg: Message) -> String {
    format!("❌ {}", msg)
}

/// Prefixes the message with the warning mark.
///
/// ```rust
/// use kasl::libs::messages::{Message, warning};
///
/// let message = warning(Message::AutostartRequiresAdmin);
/// println!("{}", message);
///
/// let data_warning = warning(Message::ShortIntervalsDetected(3, "45 minutes".to_string()));
/// println!("{}", data_warning);
/// ```
///
/// ```rust
/// use kasl::msg_warning;
/// use kasl::libs::messages::Message;
///
/// let count = 3;
/// let duration = "45 minutes".to_string();
/// msg_warning!(Message::ShortIntervalsDetected(count, duration));
/// ```
pub fn warning(msg: Message) -> String {
    format!("⚠️  {}", msg)
}

/// Prefixes the message with the info mark.
///
/// ```rust
/// use kasl::libs::messages::{Message, info};
///
/// let message = info(Message::MonitorStarted {
///     pause_threshold: 60,
///     poll_interval: 500,
///     activity_threshold: 30,
/// });
/// println!("{}", message);
///
/// let config_info = info(Message::AutostartStatus("enabled".to_string()));
/// println!("{}", config_info); // "ℹ️  Autostart is currently: enabled"
/// ```
///
/// ```rust
/// use kasl::{msg_info, msg_success};
/// use kasl::libs::messages::Message;
///
/// msg_info!(Message::WatcherStartingForeground);
/// msg_success!(Message::WatcherStarted(12345));
/// ```
pub fn info(msg: Message) -> String {
    format!("ℹ️  {}", msg)
}

/// Wraps the message in blank lines for emphasis; use sparingly.
///
/// ```rust
/// use kasl::libs::messages::{Message, wrap_msg};
///
/// let message = wrap_msg(Message::ConfirmDeleteAllTodayTasksFinal);
/// println!("{}", message);
///
/// let summary = wrap_msg(Message::AllMigrationsCompleted);
/// println!("{}", summary);
/// ```
///
/// ```rust
/// use kasl::libs::messages::{Message, success, wrap_msg};
///
/// let emphasized_success = format!("\n{}\n", success(Message::OperationCompleted));
/// let wrapped_success = wrap_msg(Message::OperationCompleted);
/// ```
pub fn wrap_msg(msg: Message) -> String {
    format!("\n{}\n", msg)
}
