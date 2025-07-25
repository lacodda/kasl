/// Convenience macros for common message operations with conditional tracing support
use std::sync::OnceLock;

static DEBUG_MODE: OnceLock<bool> = OnceLock::new();

/// Check if debug mode is enabled (cached for performance)
#[doc(hidden)]
pub fn is_debug_mode() -> bool {
    *DEBUG_MODE.get_or_init(|| std::env::var("KASL_DEBUG").is_ok() || std::env::var("RUST_LOG").is_ok())
}

/// Print a message (info level)
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

/// Print a success message with ✅ prefix
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

/// Print an error message with ❌ prefix
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

/// Print a warning message with ⚠️ prefix
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

/// Print an info message with ℹ️ prefix
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

/// Debug message - only shown when debug mode is enabled
#[macro_export]
macro_rules! msg_debug {
    ($msg:expr) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::debug!("🔍 {}", $msg);
        }
    };
}

/// Create an anyhow error from a message
#[macro_export]
macro_rules! msg_error_anyhow {
    ($msg:expr) => {
        anyhow::anyhow!("❌ {}", $msg)
    };
}

/// Bail with a message error
#[macro_export]
macro_rules! msg_bail_anyhow {
    ($msg:expr) => {
        anyhow::bail!("❌ {}", $msg)
    };
}
