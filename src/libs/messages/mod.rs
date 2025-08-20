//! Centralized message management system for kasl application.
//!
//! Provides a comprehensive message handling infrastructure that serves as the
//! foundation for all user communication in the kasl application.
//!
//! ## Features
//!
//! - **Type Safety**: All messages are compile-time verified with proper parameters
//! - **Centralization**: Single source of truth for all user-facing text
//! - **Consistency**: Uniform formatting and presentation across the application
//! - **Extensibility**: Easy addition of new message types and categories
//! - **Internationalization**: Structure supports future localization efforts
//!
//! ## Usage
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

// Re-export the main Message type for convenient access
pub use types::Message;

/// Creates a success message with a green checkmark prefix.
///
/// This function provides a standardized way to format success messages
/// throughout the application. It ensures consistent visual presentation
/// for positive feedback and completion notifications.
///
/// ## Visual Format
///
/// Success messages are prefixed with a green checkmark (✅) to provide
/// immediate visual feedback about successful operations. This creates
/// a consistent user experience across all success scenarios.
///
/// ## Usage Context
///
/// This function is typically used for:
/// - Operation completion confirmations
/// - Successful data saves or updates
/// - Configuration changes applied successfully
/// - External API communication success
/// - File operations completed without errors
///
/// # Arguments
///
/// * `msg` - A [`Message`] enum variant containing the success details
///
/// # Returns
///
/// Returns a formatted string with the success prefix and message text.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::messages::{Message, success};
///
/// // Format a simple success message
/// let message = success(Message::TaskCreated);
/// println!("{}", message); // "✅ Task created successfully"
///
/// // With parameters
/// let report_message = success(Message::DailyReportSent("2025-01-15".to_string()));
/// println!("{}", report_message); // "✅ Your report dated 2025-01-15 has been successfully submitted..."
/// ```
///
/// # Integration with Macros
///
/// This function works seamlessly with the application's messaging macros:
/// ```rust
/// use kasl::{msg_success};
///
/// // Equivalent to using success() function directly
/// msg_success!(Message::TaskCreated);
/// ```
pub fn success(msg: Message) -> String {
    format!("✅ {}", msg)
}

/// Creates an error message with a red X prefix.
///
/// This function provides a standardized way to format error messages
/// throughout the application. It ensures consistent visual presentation
/// for error conditions and failure notifications, helping users quickly
/// identify and understand problems.
///
/// ## Visual Format
///
/// Error messages are prefixed with a red X (❌) to provide immediate
/// visual indication of error conditions. This creates a consistent
/// user experience for error reporting across all application areas.
///
/// ## Error Message Philosophy
///
/// Error messages created by this function follow these principles:
/// - **Clear Problem Description**: Explain what went wrong
/// - **Helpful Context**: Provide relevant details for troubleshooting
/// - **Action Guidance**: Suggest next steps when possible
/// - **Non-Technical Language**: Avoid intimidating technical jargon
///
/// ## Usage Context
///
/// This function is typically used for:
/// - Operation failures and exceptions
/// - Validation errors and invalid input
/// - Configuration problems and conflicts
/// - External service communication failures
/// - File system and permission errors
///
/// # Arguments
///
/// * `msg` - A [`Message`] enum variant containing the error details
///
/// # Returns
///
/// Returns a formatted string with the error prefix and message text.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::messages::{Message, error};
///
/// // Format a simple error message
/// let message = error(Message::ConfigSaveError);
/// println!("{}", message); // "❌ Failed to save configuration"
///
/// // With error details
/// let detailed_error = error(Message::UpdateDownloadFailed("Network timeout".to_string()));
/// println!("{}", detailed_error); // "❌ Failed to download update: Network timeout"
/// ```
///
/// # Error Handling Integration
///
/// This function integrates with the application's error handling:
/// ```rust
/// use kasl::{msg_error};
/// use anyhow::Result;
///
/// fn save_config() -> Result<()> {
///     // ... operation that might fail
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

/// Creates a warning message with a yellow warning triangle prefix.
///
/// This function provides a standardized way to format warning messages
/// throughout the application. Warnings indicate situations that require
/// user attention but are not necessarily errors or failures.
///
/// ## Visual Format
///
/// Warning messages are prefixed with a yellow warning triangle (⚠️) to
/// indicate important information that users should be aware of. This
/// provides a visual middle ground between informational and error messages.
///
/// ## Warning Message Types
///
/// Warnings are appropriate for these scenarios:
/// - **Deprecation Notices**: Features that will be removed in future versions
/// - **Configuration Issues**: Non-critical configuration problems
/// - **Data Quality**: Potential issues with user data or inputs
/// - **Performance Notices**: Operations that might be slow or resource-intensive
/// - **Compatibility Warnings**: Version or platform compatibility concerns
///
/// ## Usage Context
///
/// This function is typically used for:
/// - Non-critical configuration issues
/// - Deprecated feature usage notifications
/// - Data quality concerns or anomalies
/// - Performance or resource usage warnings
/// - Compatibility or version mismatch notices
///
/// # Arguments
///
/// * `msg` - A [`Message`] enum variant containing the warning details
///
/// # Returns
///
/// Returns a formatted string with the warning prefix and message text.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::messages::{Message, warning};
///
/// // Format a configuration warning
/// let message = warning(Message::AutostartRequiresAdmin);
/// println!("{}", message); // "⚠️  Administrator privileges required for system-level autostart."
///
/// // Data quality warning
/// let data_warning = warning(Message::ShortIntervalsDetected(3, "45 minutes".to_string()));
/// println!("{}", data_warning); // "⚠️  Detected 3 short intervals totaling 45 minutes"
/// ```
///
/// # Integration with Application Flow
///
/// Warnings often provide guidance for resolution:
/// ```rust
/// use kasl::{msg_warning, msg_info};
///
/// // Warning with follow-up guidance
/// msg_warning!(Message::ShortIntervalsDetected(count, duration));
/// msg_info!(Message::UseReportClearCommand);
/// ```
pub fn warning(msg: Message) -> String {
    format!("⚠️  {}", msg)
}

/// Creates an informational message with a blue info icon prefix.
///
/// This function provides a standardized way to format informational messages
/// throughout the application. Info messages convey status updates, progress
/// notifications, and general information that helps users understand what
/// the application is doing.
///
/// ## Visual Format
///
/// Informational messages are prefixed with a blue info icon (ℹ️) to indicate
/// general status information that is helpful but not critical. This provides
/// clear visual categorization for different types of user feedback.
///
/// ## Information Message Types
///
/// Info messages are appropriate for these scenarios:
/// - **Status Updates**: Current operation progress and status
/// - **Configuration Information**: Details about current settings
/// - **Process Notifications**: Background process status and lifecycle
/// - **Data Statistics**: Summaries and counts of application data
/// - **Help and Guidance**: Instructional content and next steps
///
/// ## Usage Context
///
/// This function is typically used for:
/// - Background process status updates
/// - Operation progress notifications
/// - Configuration and setup information
/// - Data summary and statistics display
/// - General user guidance and tips
///
/// # Arguments
///
/// * `msg` - A [`Message`] enum variant containing the informational content
///
/// # Returns
///
/// Returns a formatted string with the info prefix and message text.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::messages::{Message, info};
///
/// // Format a status update
/// let message = info(Message::MonitorStarted {
///     pause_threshold: 60,
///     poll_interval: 500,
///     activity_threshold: 30,
/// });
/// println!("{}", message); // "ℹ️  Monitor is running with pause threshold 60s, poll interval 500ms, activity threshold 30s"
///
/// // Configuration information
/// let config_info = info(Message::AutostartStatus("enabled".to_string()));
/// println!("{}", config_info); // "ℹ️  Autostart is currently: enabled"
/// ```
///
/// # Complementary Usage
///
/// Info messages often work together with other message types:
/// ```rust
/// use kasl::{msg_info, msg_success};
///
/// // Sequential information flow
/// msg_info!(Message::WatcherStartingForeground);
/// // ... operation occurs
/// msg_success!(Message::WatcherStarted(12345));
/// ```
pub fn info(msg: Message) -> String {
    format!("ℹ️  {}", msg)
}

/// Wraps a message with newlines for emphasis and visual separation.
///
/// This function provides enhanced visual formatting for messages that require
/// special emphasis or clear separation from surrounding content. It adds
/// newlines before and after the message text to create visual whitespace
/// that draws attention to important information.
///
/// ## Visual Format
///
/// Wrapped messages are formatted with newlines on both sides:
/// ```text
///
/// Your important message here
///
/// ```
///
/// This creates clear visual separation from other content and emphasizes
/// the importance of the message.
///
/// ## Usage Context
///
/// Message wrapping is appropriate for:
/// - **Critical Announcements**: Important system notifications
/// - **Section Headers**: Major section dividers in output
/// - **Final Results**: Summary information at the end of operations
/// - **Error Emphasis**: Critical errors that require immediate attention
/// - **Interactive Prompts**: Important questions or confirmations
///
/// ## Design Considerations
///
/// Wrapped messages should be used sparingly to maintain their emphasis
/// effect. Overuse can clutter the interface and reduce the impact of
/// truly important messages.
///
/// # Arguments
///
/// * `msg` - A [`Message`] enum variant to be wrapped with emphasis
///
/// # Returns
///
/// Returns a formatted string with newlines before and after the message text.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::messages::{Message, wrap_msg};
///
/// // Emphasize a critical message
/// let message = wrap_msg(Message::ConfirmDeleteAllTodayTasksFinal);
/// println!("{}", message);
/// // Output:
/// //
/// // This action cannot be undone. Continue?
/// //
///
/// // Wrap a summary message
/// let summary = wrap_msg(Message::AllMigrationsCompleted);
/// println!("{}", summary);
/// // Output:
/// //
/// // All migrations completed successfully
/// //
/// ```
///
/// # Integration with Other Formatting
///
/// Wrapped messages can be combined with prefix functions:
/// ```rust
/// use kasl::libs::messages::{Message, success, wrap_msg};
///
/// // Create an emphasized success message
/// let emphasized_success = format!("\n{}\n", success(Message::OperationCompleted));
/// // Or use wrap_msg for consistent formatting
/// let wrapped_success = wrap_msg(success(Message::OperationCompleted));
/// ```
pub fn wrap_msg(msg: Message) -> String {
    format!("\n{}\n", msg)
}
