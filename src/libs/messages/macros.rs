//! Print macros that route to the console or to `tracing`.
//!
//! Every macro checks [`is_debug_mode`] once per call: with `KASL_DEBUG` or
//! `RUST_LOG` set, output goes through `tracing` (so the daemon's log captures
//! it); otherwise it goes straight to stdout/stderr. Each macro takes an
//! optional `true` second argument to pad the message with blank lines.
//!
//! ```rust
//! use kasl::{msg_info, msg_error, msg_success, msg_warning};
//! use kasl::libs::messages::types::Message;
//!
//! msg_info!(Message::TaskCreated);
//! msg_success!(Message::DailyReportSent("2025-01-15".to_string()));
//! msg_error!(Message::ConfigSaveError);
//!
//! let count = 5;
//! msg_info!(format!("Processing {} items", count));
//! ```

use std::sync::OnceLock;

/// Cached result of the environment check; env vars are read once per run.
static DEBUG_MODE: OnceLock<bool> = OnceLock::new();

/// True when `KASL_DEBUG` or `RUST_LOG` is set.
///
/// ```rust
/// use kasl::libs::messages::macros::is_debug_mode;
///
/// if is_debug_mode() {
///     println!("Running in debug mode with enhanced logging");
/// } else {
///     println!("Running in normal mode with simple output");
/// }
/// ```
#[doc(hidden)]
pub fn is_debug_mode() -> bool {
    *DEBUG_MODE.get_or_init(|| std::env::var("KASL_DEBUG").is_ok() || std::env::var("RUST_LOG").is_ok())
}

/// Prints the message with no prefix.
///
/// ```rust
/// use kasl::msg_print;
/// use kasl::libs::messages::types::Message;
///
/// msg_print!(Message::ConfigSaved);
/// ```
///
/// ```rust
/// use kasl::msg_print;
/// use kasl::libs::messages::types::Message;
///
/// msg_print!(Message::ReportHeader("2025-01-15".to_string()), true);
/// ```
#[macro_export]
macro_rules! msg_print {
    ($msg:expr) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::info!("{}", $msg);
        } else {
            println!("{}", $msg);
        }
    };
    ($msg:expr, true) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::info!("\n{}\n", $msg);
        } else {
            println!("\n{}\n", $msg);
        }
    };
}

/// Prints the message with the ✅ prefix.
///
/// ```rust
/// use kasl::msg_success;
/// use kasl::libs::messages::types::Message;
///
/// msg_success!(Message::TaskCreated);
/// ```
///
/// ```rust
/// use kasl::msg_success;
/// use kasl::libs::messages::types::Message;
///
/// msg_success!(Message::ExportCompleted("data.csv".to_string()), true);
/// ```
#[macro_export]
macro_rules! msg_success {
    ($msg:expr) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::info!("✅ {}", $msg);
        } else {
            println!("✅ {}", $msg);
        }
    };
    ($msg:expr, true) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::info!("\n✅ {}\n", $msg);
        } else {
            println!("\n✅ {}\n", $msg);
        }
    };
}

/// Prints the message with the ❌ prefix - to stderr outside debug mode, so
/// errors stay separable from data in pipes.
///
/// ```rust
/// use kasl::msg_error;
/// use kasl::libs::messages::types::Message;
///
/// msg_error!(Message::TaskNotFound);
/// ```
///
/// ```rust
/// use kasl::msg_error;
/// use kasl::libs::messages::types::Message;
///
/// msg_error!(Message::ConfigParseError, true);
/// ```
#[macro_export]
macro_rules! msg_error {
    ($msg:expr) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::error!("❌ {}", $msg);
        } else {
            eprintln!("❌ {}", $msg);
        }
    };
    ($msg:expr, true) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::error!("\n❌ {}\n", $msg);
        } else {
            eprintln!("\n❌ {}\n", $msg);
        }
    };
}

/// Prints the message with the ⚠️ prefix.
///
/// ```rust
/// use kasl::msg_warning;
/// use kasl::libs::messages::types::Message;
///
/// msg_warning!(Message::AutostartCheckingAlternative);
/// ```
///
/// ```rust
/// use kasl::msg_warning;
/// use kasl::libs::messages::types::Message;
///
/// msg_warning!(Message::WatcherSignalHandlingNotSupported, true);
/// ```
#[macro_export]
macro_rules! msg_warning {
    ($msg:expr) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::warn!("⚠️ {}", $msg);
        } else {
            println!("⚠️ {}", $msg);
        }
    };
    ($msg:expr, true) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::warn!("\n⚠️ {}\n", $msg);
        } else {
            println!("\n⚠️ {}\n", $msg);
        }
    };
}

/// Prints the message with the ℹ️ prefix.
///
/// ```rust
/// use kasl::msg_info;
/// use kasl::libs::messages::types::Message;
///
/// msg_info!(Message::WatcherStarted(1234));
/// ```
///
/// ```rust
/// use kasl::msg_info;
/// use kasl::libs::messages::types::Message;
///
/// msg_info!(Message::WorkingHoursForMonth("2025-01".to_string()), true);
/// ```
#[macro_export]
macro_rules! msg_info {
    ($msg:expr) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::info!("ℹ️ {}", $msg);
        } else {
            println!("ℹ️ {}", $msg);
        }
    };
    ($msg:expr, true) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::info!("\nℹ️ {}\n", $msg);
        } else {
            println!("\nℹ️ {}\n", $msg);
        }
    };
}

/// Logs the message with the 🔍 prefix in debug mode; silent otherwise.
///
/// ```rust
/// use kasl::msg_debug;
///
/// let task_id = 42;
/// msg_debug!(format!("Processing task with ID: {}", task_id));
/// ```
///
/// ```rust
/// use kasl::msg_debug;
///
/// let old_state = "Active";
/// let new_state = "InPause";
/// msg_debug!(format!("State transition: {:?} -> {:?}", old_state, new_state));
/// ```
#[macro_export]
macro_rules! msg_debug {
    ($msg:expr) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::debug!("🔍 {}", $msg);
        }
    };
}

/// Builds an `anyhow::Error` from the message, ❌-prefixed.
///
/// ```rust
/// use anyhow::Result;
/// use kasl::{msg_error_anyhow, libs::messages::Message};
///
/// # fn config_is_invalid() -> bool { false }
/// fn validate_config() -> Result<()> {
///     if config_is_invalid() {
///         return Err(msg_error_anyhow!(Message::ConfigParseError));
///     }
///     Ok(())
/// }
/// ```
///
/// ```rust
/// use anyhow::{Result, Context};
/// use kasl::{msg_error_anyhow, libs::messages::Message};
///
/// # fn some_operation() -> Result<()> { Ok(()) }
/// fn complex_operation() -> Result<()> {
///     some_operation()
///         .context(msg_error_anyhow!(Message::TaskUpdateFailed))
/// }
/// ```
#[macro_export]
macro_rules! msg_error_anyhow {
    ($msg:expr) => {
        anyhow::anyhow!("❌ {}", $msg)
    };
}

/// `return Err(...)` with the message, ❌-prefixed.
///
/// ```rust
/// use anyhow::Result;
/// use kasl::{msg_bail_anyhow, libs::messages::Message};
///
/// fn process_task(task_id: Option<i32>) -> Result<()> {
///     let id = match task_id {
///         Some(id) => id,
///         None => msg_bail_anyhow!(Message::InvalidInput),
///     };
///     let _ = id;
///     Ok(())
/// }
/// ```
///
/// ```rust
/// use anyhow::Result;
/// use kasl::{msg_bail_anyhow, libs::messages::Message};
///
/// # fn user_has_permission() -> bool { true }
/// fn secure_operation() -> Result<()> {
///     if !user_has_permission() {
///         msg_bail_anyhow!(Message::PermissionDenied);
///     }
///     Ok(())
/// }
/// ```
///
/// ```rust
/// use anyhow::Result;
/// use kasl::{msg_bail_anyhow, libs::messages::Message};
///
/// # fn resource_exists(_path: &str) -> bool { true }
/// fn access_resource(path: &str) -> Result<()> {
///     if !resource_exists(path) {
///         msg_bail_anyhow!(Message::FileNotFound);
///     }
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! msg_bail_anyhow {
    ($msg:expr) => {
        anyhow::bail!("❌ {}", $msg)
    };
}
