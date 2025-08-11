//! Convenient macros for application messaging and logging.
//!
//! This module provides a comprehensive set of macros that simplify message
//! display and logging throughout the application. The macros automatically
//! handle the distinction between debug mode (with structured logging) and
//! normal mode (with simple console output), providing a unified interface
//! for all message display needs.
//!
//! ## Core Features
//!
//! - **Dual Output Mode**: Automatic switching between tracing and console output
//! - **Debug Detection**: Runtime detection of debug mode configuration
//! - **Message Categorization**: Different macros for different message types
//! - **Performance Optimization**: Cached debug mode detection for efficiency
//! - **Error Handling**: Specialized macros for error creation and handling
//! - **Flexible Formatting**: Support for both simple and formatted message display
//!
//! ## Debug Mode Detection
//!
//! The system automatically detects debug mode based on environment variables:
//! - **`KASL_DEBUG`**: Explicit debug mode enablement
//! - **`RUST_LOG`**: Standard Rust logging configuration
//! - **Caching**: Debug mode detection is cached for performance
//!
//! ## Output Routing
//!
//! ```text
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │   Macro Call    │    │   Debug Mode    │    │   Output        │
//! │   msg_info!()   │───▶│   Detection     │───▶│   Routing       │
//! │                 │    │                 │    │                 │
//! └─────────────────┘    └─────────────────┘    └─────────────────┘
//!          │                       │                       │
//!          ▼                       ▼                       ▼
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │ Message Content │    │ KASL_DEBUG or   │    │ tracing::info!  │
//! │ + Level Info    │    │ RUST_LOG Set?   │    │ OR println!     │
//! └─────────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! ## Macro Categories
//!
//! ### Display Macros
//! - **`msg_print!`**: General message display
//! - **`msg_success!`**: Success notifications with ✅ prefix
//! - **`msg_info!`**: Informational messages with ℹ️ prefix
//! - **`msg_warning!`**: Warning messages with ⚠️ prefix
//!
//! ### Error Handling Macros
//! - **`msg_error!`**: Error messages with ❌ prefix
//! - **`msg_error_anyhow!`**: Create anyhow::Error from messages
//! - **`msg_bail_anyhow!`**: Early return with error
//!
//! ### Debug Macros
//! - **`msg_debug!`**: Debug-only messages with 🔍 prefix
//!
//! ## Performance Considerations
//!
//! - **Lazy Evaluation**: Debug mode detection is cached on first use
//! - **Conditional Compilation**: Debug messages can be conditionally compiled
//! - **Minimal Overhead**: Fast path for production mode with simple println!
//! - **Efficient Caching**: Environment variable checks are cached using OnceLock
//!
//! ## Usage Examples
//!
//! ### Basic Message Display
//! ```rust
//! use kasl::{msg_info, msg_success, msg_error};
//! use kasl::libs::messages::Message;
//!
//! // Simple success message
//! msg_success!(Message::TaskCreated);
//!
//! // Informational message with line breaks
//! msg_info!(Message::ConfigSaved, true);
//!
//! // Error message
//! msg_error!(Message::TaskNotFound);
//! ```
//!
//! ### Error Handling
//! ```rust
//! use kasl::{msg_error_anyhow, msg_bail_anyhow};
//! use kasl::libs::messages::Message;
//!
//! // Create an error for propagation
//! let error = msg_error_anyhow!(Message::ConfigParseError);
//!
//! // Early return with error
//! msg_bail_anyhow!(Message::PermissionDenied);
//! ```
//!
//! ### Debug Logging
//! ```rust
//! use kasl::msg_debug;
//!
//! // Debug message (only shown when KASL_DEBUG is set)
//! msg_debug!("Processing task with ID: {}", task_id);
//! ```

/// Convenience macros for common message operations with conditional tracing support
use std::sync::OnceLock;

/// Global cache for debug mode detection to avoid repeated environment variable checks.
///
/// This static variable uses `OnceLock` to cache the result of debug mode detection
/// on first access. This provides significant performance benefits by avoiding
/// repeated environment variable lookups, which can be expensive operations.
///
/// ## Performance Benefits
/// - **Single Check**: Environment variables are checked only once per application run
/// - **Fast Access**: Subsequent checks are simple memory reads
/// - **Thread Safety**: OnceLock provides thread-safe initialization
/// - **Memory Efficiency**: Minimal memory overhead for caching
static DEBUG_MODE: OnceLock<bool> = OnceLock::new();

/// Checks if debug mode is enabled, with caching for performance.
///
/// This function determines whether the application is running in debug mode
/// by checking for the presence of debug-related environment variables. The
/// result is cached using `OnceLock` to avoid repeated expensive environment
/// variable lookups.
///
/// ## Detection Logic
///
/// Debug mode is considered enabled if either of these environment variables is set:
/// - **`KASL_DEBUG`**: Application-specific debug flag
/// - **`RUST_LOG`**: Standard Rust logging configuration
///
/// The presence of either variable indicates that the user wants enhanced
/// logging output and expects debug information to be available.
///
/// ## Caching Strategy
///
/// The function uses a lazy initialization pattern:
/// 1. **First Call**: Checks environment variables and caches result
/// 2. **Subsequent Calls**: Returns cached value without environment checks
/// 3. **Thread Safety**: Multiple threads can safely call this function
/// 4. **Performance**: Subsequent calls are essentially free
///
/// ## Integration Points
///
/// This function is used by all message macros to determine output routing:
/// - **Debug Mode**: Messages go to tracing system with structured logging
/// - **Normal Mode**: Messages go to simple console output (println!/eprintln!)
///
/// # Returns
///
/// Returns `true` if debug mode is enabled, `false` otherwise. The result
/// is cached for the lifetime of the application.
///
/// # Examples
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
    *DEBUG_MODE.get_or_init(|| {
        // Check for application-specific debug flag
        std::env::var("KASL_DEBUG").is_ok() ||
        // Check for standard Rust logging configuration
        std::env::var("RUST_LOG").is_ok()
    })
}

/// Prints a general message with automatic debug mode routing.
///
/// This macro provides the basic message display functionality with automatic
/// detection of debug mode to route output appropriately. It supports both
/// simple single-line messages and formatted messages with optional line breaks.
///
/// ## Output Routing
///
/// - **Debug Mode**: Uses `tracing::info!` for structured logging
/// - **Normal Mode**: Uses `println!` for simple console output
///
/// ## Usage Patterns
///
/// ### Simple Message
/// ```rust
/// msg_print!(Message::ConfigSaved);
/// // Output: "Configuration saved successfully"
/// ```
///
/// ### Message with Line Breaks
/// ```rust
/// msg_print!(Message::ReportHeader, true);
/// // Output: "\n📊 Daily Work Report\n"
/// ```
///
/// ## Performance Notes
///
/// - Debug mode detection is cached for efficiency
/// - Tracing integration provides structured logging in debug mode
/// - Simple println! provides fast output in production mode
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

/// Prints a success message with ✅ prefix and automatic routing.
///
/// This macro is specifically designed for displaying success notifications
/// and positive confirmations. The green checkmark emoji provides visual
/// confirmation that operations completed successfully.
///
/// ## Visual Design
///
/// - **Prefix**: ✅ (green checkmark emoji)
/// - **Purpose**: Success confirmations and positive outcomes
/// - **Examples**: Task creation, configuration saves, successful exports
///
/// ## Output Examples
///
/// ```text
/// ✅ Task created successfully
/// ✅ Configuration saved successfully
/// ✅ Data exported to file.csv
/// ```
///
/// ## Usage Patterns
///
/// ### Simple Success Message
/// ```rust
/// msg_success!(Message::TaskCreated);
/// // Output: "✅ Task created successfully"
/// ```
///
/// ### Success Message with Line Breaks
/// ```rust
/// msg_success!(Message::ExportSuccess("data.csv".to_string()), true);
/// // Output: "\n✅ Data exported successfully to: data.csv\n"
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

/// Prints an error message with ❌ prefix and automatic routing.
///
/// This macro handles error message display with appropriate severity level
/// routing. In debug mode, errors are logged through the tracing system,
/// while in normal mode they're displayed on stderr for proper error handling.
///
/// ## Visual Design
///
/// - **Prefix**: ❌ (red X emoji)
/// - **Purpose**: Error notifications and failure messages
/// - **Stream**: Uses stderr in normal mode for proper error stream handling
///
/// ## Output Routing
///
/// - **Debug Mode**: Uses `tracing::error!` for structured error logging
/// - **Normal Mode**: Uses `eprintln!` to write to stderr
///
/// ## Error Stream Benefits
///
/// Using stderr for error output provides several advantages:
/// - **Stream Separation**: Errors don't interfere with normal output
/// - **Script Compatibility**: Scripts can separate errors from data
/// - **Shell Redirection**: Users can redirect errors independently
/// - **Log Aggregation**: Error logs can be collected separately
///
/// ## Usage Patterns
///
/// ### Simple Error Message
/// ```rust
/// msg_error!(Message::TaskNotFound);
/// // Output to stderr: "❌ Task not found"
/// ```
///
/// ### Error Message with Line Breaks
/// ```rust
/// msg_error!(Message::ConfigParseError, true);
/// // Output to stderr: "\n❌ Failed to parse configuration file\n"
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

/// Prints a warning message with ⚠️ prefix and automatic routing.
///
/// This macro displays warning messages that indicate potential issues or
/// situations requiring user attention, but which don't prevent operation
/// from continuing. Warnings help users understand system state and make
/// informed decisions.
///
/// ## Visual Design
///
/// - **Prefix**: ⚠️ (warning triangle emoji)
/// - **Purpose**: Cautionary messages and non-critical issues
/// - **Severity**: Less critical than errors, more important than info
///
/// ## Warning Categories
///
/// Warnings are appropriate for:
/// - **Deprecated Features**: Features that will be removed in future versions
/// - **Configuration Issues**: Non-critical configuration problems
/// - **Performance Concerns**: Operations that may be slow or inefficient
/// - **Fallback Behavior**: When the system falls back to default behavior
/// - **Resource Limitations**: When approaching resource limits
///
/// ## Usage Patterns
///
/// ### Simple Warning Message
/// ```rust
/// msg_warning!(Message::AutostartCheckingAlternative);
/// // Output: "⚠️ Checking alternative autostart methods..."
/// ```
///
/// ### Warning Message with Line Breaks
/// ```rust
/// msg_warning!(Message::SignalHandlingNotSupported, true);
/// // Output: "\n⚠️ Signal handling not supported on this platform\n"
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

/// Prints an informational message with ℹ️ prefix and automatic routing.
///
/// This macro displays informational messages that provide useful context
/// or status updates to users. Info messages help users understand what
/// the system is doing and provide transparency into system operations.
///
/// ## Visual Design
///
/// - **Prefix**: ℹ️ (information emoji)
/// - **Purpose**: Status updates and informational content
/// - **Tone**: Neutral, informative, helpful
///
/// ## Information Categories
///
/// Info messages are appropriate for:
/// - **Status Updates**: Progress information for long-running operations
/// - **System State**: Current system status and configuration
/// - **Process Information**: What the system is currently doing
/// - **User Guidance**: Helpful tips and usage information
/// - **Confirmation**: Non-critical confirmations and acknowledgments
///
/// ## Usage Patterns
///
/// ### Simple Info Message
/// ```rust
/// msg_info!(Message::WatcherStarted(1234));
/// // Output: "ℹ️ Watcher started with PID: 1234"
/// ```
///
/// ### Info Message with Line Breaks
/// ```rust
/// msg_info!(Message::MonthlySummaryHeader, true);
/// // Output: "\nℹ️ 📅 Monthly Summary\n"
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

/// Debug-only message display with 🔍 prefix.
///
/// This macro provides debug-specific logging that only appears when debug
/// mode is explicitly enabled. Debug messages are useful for troubleshooting
/// and development but are hidden from normal users to avoid clutter.
///
/// ## Debug-Only Behavior
///
/// - **Debug Mode**: Messages are displayed using `tracing::debug!`
/// - **Normal Mode**: Messages are completely suppressed (no output)
/// - **Performance**: No overhead in production builds when debug is disabled
///
/// ## Visual Design
///
/// - **Prefix**: 🔍 (magnifying glass emoji)
/// - **Purpose**: Development and troubleshooting information
/// - **Audience**: Developers and power users debugging issues
///
/// ## Debug Message Categories
///
/// Debug messages are appropriate for:
/// - **Technical Details**: Low-level system information
/// - **State Changes**: Internal state transitions and updates
/// - **Performance Metrics**: Timing and performance measurements
/// - **Data Flow**: How data moves through the system
/// - **Error Context**: Additional context for debugging errors
///
/// ## Usage Patterns
///
/// ### Technical Debug Information
/// ```rust
/// msg_debug!("Processing task with ID: {}", task_id);
/// // Debug mode output: "🔍 Processing task with ID: 42"
/// // Normal mode output: (nothing)
/// ```
///
/// ### State Change Debugging
/// ```rust
/// msg_debug!(format!("State transition: {:?} -> {:?}", old_state, new_state));
/// // Debug mode output: "🔍 State transition: Active -> InPause"
/// // Normal mode output: (nothing)
/// ```
#[macro_export]
macro_rules! msg_debug {
    ($msg:expr) => {
        if $crate::libs::messages::macros::is_debug_mode() {
            tracing::debug!("🔍 {}", $msg);
        }
    };
}

/// Creates an `anyhow::Error` from a message with ❌ prefix.
///
/// This macro provides a convenient way to create `anyhow::Error` instances
/// from application messages. It's useful for error propagation in functions
/// that return `Result<T, anyhow::Error>` and need to convert application
/// messages into proper error types.
///
/// ## Error Creation Strategy
///
/// - **Prefix Addition**: Automatically adds ❌ prefix for visual consistency
/// - **Error Propagation**: Creates errors suitable for `?` operator use
/// - **Message Integration**: Works with the application's message system
/// - **Type Compatibility**: Returns `anyhow::Error` for easy integration
///
/// ## Use Cases
///
/// ### Function Error Returns
/// ```rust
/// use anyhow::Result;
/// use kasl::{msg_error_anyhow, libs::messages::Message};
///
/// fn validate_config() -> Result<()> {
///     if config_is_invalid() {
///         return Err(msg_error_anyhow!(Message::ConfigParseError));
///     }
///     Ok(())
/// }
/// ```
///
/// ### Error Context Addition
/// ```rust
/// use anyhow::{Result, Context};
///
/// fn complex_operation() -> Result<()> {
///     some_operation()
///         .context(msg_error_anyhow!(Message::OperationFailed))
/// }
/// ```
///
/// ## Error Handling Benefits
///
/// - **Consistent Formatting**: All errors have consistent visual presentation
/// - **Message Reuse**: Leverages existing message definitions
/// - **Type Safety**: Provides proper error types for Rust's error handling
/// - **Integration**: Works seamlessly with `anyhow` and `?` operator
#[macro_export]
macro_rules! msg_error_anyhow {
    ($msg:expr) => {
        anyhow::anyhow!("❌ {}", $msg)
    };
}

/// Early return with an error created from a message.
///
/// This macro combines error creation with immediate return, providing a
/// convenient way to exit functions early when error conditions are detected.
/// It's equivalent to `return Err(msg_error_anyhow!(message))` but more concise.
///
/// ## Early Return Pattern
///
/// - **Error Creation**: Creates an `anyhow::Error` with ❌ prefix
/// - **Immediate Return**: Returns the error immediately from the function
/// - **Function Exit**: Stops execution at the point of the macro call
/// - **Clean Code**: Reduces boilerplate for error handling
///
/// ## Use Cases
///
/// ### Input Validation
/// ```rust
/// use anyhow::Result;
/// use kasl::{msg_bail_anyhow, libs::messages::Message};
///
/// fn process_task(task_id: Option<i32>) -> Result<()> {
///     let id = task_id.unwrap_or_else(|| {
///         msg_bail_anyhow!(Message::InvalidInput);
///     });
///    
///     // Continue processing with valid ID
///     Ok(())
/// }
/// ```
///
/// ### Permission Checking
/// ```rust
/// fn secure_operation() -> Result<()> {
///     if !user_has_permission() {
///         msg_bail_anyhow!(Message::PermissionDenied);
///     }
///    
///     // Continue with authorized operation
///     Ok(())
/// }
/// ```
///
/// ### Resource Validation
/// ```rust
/// fn access_resource(path: &str) -> Result<()> {
///     if !resource_exists(path) {
///         msg_bail_anyhow!(Message::ResourceNotFound);
///     }
///    
///     // Continue with valid resource
///     Ok(())
/// }
/// ```
///
/// ## Code Style Benefits
///
/// - **Reduced Boilerplate**: Eliminates repetitive error handling code
/// - **Clear Intent**: Makes error conditions immediately obvious
/// - **Consistent Errors**: All bail errors have consistent formatting
/// - **Maintainability**: Easier to update error handling patterns
#[macro_export]
macro_rules! msg_bail_anyhow {
    ($msg:expr) => {
        anyhow::bail!("❌ {}", $msg)
    };
}
