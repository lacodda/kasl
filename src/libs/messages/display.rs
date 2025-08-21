//! Display implementation for kasl application messages.
//!
//! This module provides the core `Display` trait implementation for the `Message`
//! enum, enabling automatic conversion of structured message data into human-readable
//! text suitable for terminal output. It serves as the central localization and
//! text formatting system for all user-facing messages in the kasl application.
//!
//! ## Architecture Overview
//!
//! The display system follows a centralized message management approach:
//! - **Single Source of Truth**: All message text is defined in one location
//! - **Type Safety**: Compile-time verification of message parameter usage
//! - **Internationalization Ready**: Structured for future localization support
//! - **Consistent Formatting**: Uniform message presentation across the application
//! - **Parameter Interpolation**: Safe string formatting with typed parameters
//!
//! ## Message Categories
//!
//! The implementation handles these message categories:
//! - **Autostart Messages**: System boot integration status and operations
//! - **Task Messages**: Work item management, creation, updates, and deletion
//! - **Workday Messages**: Daily work session lifecycle and management
//! - **Configuration Messages**: Setup, validation, and module configuration
//! - **Report Messages**: Daily and monthly report generation and submission
//! - **Export Messages**: Data export operations and format handling
//! - **Template Messages**: Task template management and operations
//! - **Tag Messages**: Organizational tag management and assignment
//! - **Monitor Messages**: Activity monitoring and daemon process management
//! - **Pause Messages**: Break detection and timing management
//! - **Update Messages**: Application update checking and installation
//! - **Error Messages**: Comprehensive error reporting and troubleshooting
//!
//! ## Text Formatting Standards
//!
//! All message text follows consistent formatting guidelines:
//! - **Sentence Case**: Natural capitalization for readability
//! - **Active Voice**: Clear, direct communication style
//! - **Specific Details**: Include relevant context and parameters
//! - **Professional Tone**: Appropriate for business and personal use
//! - **Action Guidance**: Clear next steps when applicable
//!
//! ## Parameter Interpolation
//!
//! Messages with dynamic content use safe parameter interpolation:
//! ```rust
//! // Type-safe parameter usage
//! Message::TaskCreated(name) => format!("Task '{}' created successfully", name)
//! Message::WorkdayStarting(date) => format!("Starting workday for {}", date)
//! Message::ExportCompleted(path) => format!("Export completed: {}", path)
//! ```
//!
//! ## Error Message Design
//!
//! Error messages follow specific principles:
//! - **Problem Description**: Clear explanation of what went wrong
//! - **Context Information**: Relevant details for troubleshooting
//! - **Resolution Guidance**: Suggested actions when possible
//! - **Technical Details**: Specific error codes or system information
//!
//! ## Usage Integration
//!
//! The display system integrates with kasl's messaging macros:
//! ```rust
//! use kasl::{msg_info, msg_error, msg_success};
//!
//! // Automatic message formatting
//! msg_info!(Message::TaskCreated);
//! msg_error!(Message::ConfigSaveError);
//! msg_success!(Message::ReportSent(date));
//! ```
//!
//! ## Future Extensibility
//!
//! The centralized design supports future enhancements:
//! - **Internationalization**: Replace text with locale-specific versions
//! - **Rich Formatting**: Add terminal colors, bold, and other styling
//! - **Context Awareness**: Modify messages based on user preferences
//! - **Help Integration**: Link messages to relevant documentation

use super::types::Message;
use std::fmt::{Display, Formatter, Result};

impl Display for Message {
    /// Converts a `Message` enum variant into human-readable text.
    ///
    /// This method implements the core message-to-text conversion logic for
    /// the entire kasl application. It provides consistent, professional
    /// text formatting for all user-facing messages while maintaining
    /// type safety and parameter interpolation.
    ///
    /// ## Implementation Strategy
    ///
    /// The method uses a comprehensive match statement to handle each message
    /// variant individually, ensuring that:
    /// - All message types are explicitly handled
    /// - Parameter interpolation is type-safe
    /// - Text formatting is consistent across message categories
    /// - New message types require explicit formatting decisions
    ///
    /// ## Text Quality Standards
    ///
    /// All generated text adheres to these quality standards:
    /// - **Clarity**: Messages are easily understood by users
    /// - **Specificity**: Include relevant details and context
    /// - **Actionability**: Provide guidance for next steps when appropriate
    /// - **Professionalism**: Suitable for business and personal environments
    /// - **Consistency**: Uniform tone and style across all messages
    ///
    /// ## Parameter Handling
    ///
    /// Messages with parameters use safe string interpolation:
    /// - String parameters are inserted directly
    /// - Numeric parameters are formatted appropriately
    /// - Collections are joined with appropriate separators
    /// - Optional values are handled with meaningful defaults
    ///
    /// ## Error Message Philosophy
    ///
    /// Error messages are designed to be helpful rather than technical:
    /// - Focus on user-understandable problems
    /// - Suggest concrete resolution steps
    /// - Avoid intimidating technical jargon
    /// - Provide sufficient context for troubleshooting
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter for writing the text output
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the message was successfully formatted,
    /// or an error if the formatting operation fails.
    ///
    /// # Error Scenarios
    ///
    /// - **Formatter Errors**: Underlying write operations fail
    /// - **Memory Allocation**: Insufficient memory for string operations
    /// - **Parameter Formatting**: Invalid parameter values (rare)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kasl::libs::messages::Message;
    ///
    /// // Automatic formatting through Display trait
    /// let message = Message::TaskCreated;
    /// println!("{}", message); // "Task created successfully"
    ///
    /// // With parameters
    /// let date_message = Message::WorkdayStarting("2025-01-15".to_string());
    /// println!("{}", date_message); // "Starting workday for 2025-01-15"
    /// ```
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let text = match self {
            // === AUTOSTART MESSAGES ===
            Message::AutostartEnabled => "Autostart has been enabled. Kasl will start automatically on system boot.".to_string(),
            Message::AutostartEnabledUser => "Autostart has been enabled for current user. Kasl will start when you log in.".to_string(),
            Message::AutostartDisabled => "Autostart has been disabled.".to_string(),
            Message::AutostartAlreadyDisabled => "Autostart was already disabled.".to_string(),
            Message::AutostartEnableFailed(error) => format!("Failed to enable autostart: {}", error),
            Message::AutostartDisableFailed(error) => format!("Failed to disable autostart: {}", error),
            Message::AutostartStatus(status) => format!("Autostart is currently: {}", status),
            Message::AutostartNotImplemented => "Autostart is not yet implemented for this operating system.".to_string(),
            Message::AutostartRequiresAdmin => "Administrator privileges required for system-level autostart. Trying user-level alternative...".to_string(),
            Message::AutostartCheckingAlternative => "Checking alternative autostart method...".to_string(),

            // === TASK MESSAGES ===
            Message::TaskCreated => "Task created successfully".to_string(),
            Message::TaskUpdated => "Task updated successfully".to_string(),
            Message::TaskDeleted => "Task deleted successfully".to_string(),
            Message::TaskNotFound => "Task not found".to_string(),
            Message::TaskCreateFailed => "Failed to create task".to_string(),
            Message::TaskUpdateFailed => "Failed to update task".to_string(),
            Message::TaskDeleteFailed => "Failed to delete task".to_string(),
            Message::TasksDeletedCount(count) => format!("Deleted {} task(s) successfully.", count),
            Message::TasksNotFoundForDate(date) => format!("Tasks not found for {}, report not sent.", date),
            Message::TasksNotFoundSad => "Tasks not found((".to_string(),
            Message::TasksHeader => "Tasks:".to_string(),
            Message::TasksIncompleteHeader => "Incomplete tasks".to_string(),
            Message::TasksGitlabHeader => "Gitlab commits".to_string(),
            Message::TasksJiraHeader => "Jira issues".to_string(),
            Message::NoTaskIdsProvided => "No task IDs provided for deletion.".to_string(),
            Message::TasksNotFoundForIds(ids) => format!("No tasks found with IDs: {:?}", ids),
            Message::TasksToBeDeleted => "The following tasks will be deleted:".to_string(),
            Message::ConfirmDeleteTask => "Are you sure you want to delete this task?".to_string(),
            Message::ConfirmDeleteTasks(count) => format!("Are you sure you want to delete {} tasks?", count),
            Message::ConfirmDeleteAllTodayTasks(count) => format!("Are you sure you want to delete ALL {} tasks for today?", count),
            Message::ConfirmDeleteAllTodayTasksFinal => "This action cannot be undone. Are you REALLY sure?".to_string(),
            Message::NoTasksForToday => "No tasks found for today.".to_string(),
            Message::TaskNotFoundWithId(id) => format!("Task with ID {} not found.", id),
            Message::CurrentTaskState => "Current task:".to_string(),
            Message::TaskEditPreview => "Task after changes:".to_string(),
            Message::ConfirmTaskUpdate => "Save changes?".to_string(),
            Message::NoChangesDetected => "No changes detected.".to_string(),
            Message::NoTasksSelected => "No tasks selected for editing.".to_string(),
            Message::SelectTasksToEdit => "Select tasks to edit (space to select, enter to confirm)".to_string(),
            Message::EditingTask(name) => format!("Editing task: {}", name),
            Message::TaskUpdatedWithName(name) => format!("Task '{}' updated successfully.", name),
            Message::TaskSkippedNoChanges(name) => format!("Task '{}' - no changes, skipped.", name),
            Message::TaskEditingCompleted => "Task editing completed.".to_string(),
            Message::PromptTaskNameEdit => "Task name".to_string(),
            Message::PromptTaskCommentEdit => "Comment (optional)".to_string(),
            Message::PromptTaskCompletenessEdit => "Completeness (0-100)".to_string(),
            Message::TaskCompletenessRange => "Completeness must be between 0 and 100".to_string(),

            // === WORKDAY MESSAGES ===
            Message::WorkdayEnded => "Workday ended for today.".to_string(),
            Message::WorkdayNotFound => "No workday record found".to_string(),
            Message::WorkdayNotFoundForDate(date) => format!("No workday record found for {}", date),
            Message::WorkdayCreateFailed => "Failed to create workday".to_string(),
            Message::WorkdayStarting(date) => format!("Starting workday for {}", date),
            Message::WorkdayCouldNotFindAfterFinalizing(date) => {
                format!("Could not find workday for {} after finalizing.", date)
            }

            // === CONFIGURATION MESSAGES ===
            Message::ConfigSaved => "Configuration saved successfully".to_string(),
            Message::ConfigLoaded => "Configuration loaded successfully".to_string(),
            Message::ConfigFileNotFound => "Configuration file not found".to_string(),
            Message::ConfigParseError => "Failed to parse configuration".to_string(),
            Message::ConfigSaveError => "Failed to save configuration".to_string(),
            Message::ConfigModuleGitLab => "GitLab settings".to_string(),
            Message::ConfigModuleJira => "Jira settings".to_string(),
            Message::ConfigModuleSiServer => "SiServer settings".to_string(),
            Message::ConfigModuleMonitor => "Monitor settings".to_string(),
            Message::ConfigModuleServer => "Server settings".to_string(),

            // === REPORT MESSAGES ===
            Message::DailyReportSent(date) => {
                format!(
                    "Your report dated {} has been successfully submitted\nWait for a message to your email address",
                    date
                )
            }
            Message::MonthlyReportSent(date) => {
                format!(
                    "Your monthly report dated {} has been successfully submitted\nWait for a message to your email address",
                    date
                )
            }
            Message::MonthlyReportTriggered => "It's the last working day of the month. Submitting the monthly report as well...".to_string(),
            Message::ReportSendFailed(status) => format!("Failed to send report. Status: {}", status),
            Message::MonthlyReportSendFailed(status) => format!("Failed to send monthly report. Status: {}", status),
            Message::ReportHeader(date) => format!("Report for {}", date),
            Message::WorkingHoursForMonth(month_year) => format!("Working hours for {}", month_year),

            // === EXPORT MESSAGES ===
            Message::ExportingData(data, format) => format!("Exporting {} in {} format...", data, format),
            Message::ExportCompleted(path) => format!("Export completed successfully: {}", path),
            Message::ExportingAllData => "Exporting all data...".to_string(),
            Message::ExportFailed(error) => format!("Export failed: {}", error),

            // === TEMPLATE MESSAGES ===
            Message::TemplateCreated(name) => format!("Template '{}' created successfully.", name),
            Message::TemplateUpdated(name) => format!("Template '{}' updated successfully.", name),
            Message::TemplateDeleted(name) => format!("Template '{}' deleted successfully.", name),
            Message::TemplateNotFound(name) => format!("Template '{}' not found.", name),
            Message::TemplateAlreadyExists(name) => format!("Template '{}' already exists.", name),
            Message::TemplateCreateFailed => "Failed to create template.".to_string(),
            Message::NoTemplatesFound => "No templates found.".to_string(),
            Message::TemplateListHeader => "Task Templates:".to_string(),
            Message::SelectTemplateToEdit => "Select template to edit".to_string(),
            Message::SelectTemplateToDelete => "Select template to delete".to_string(),
            Message::ConfirmDeleteTemplate(name) => format!("Delete template '{}'?", name),
            Message::EditingTemplate(name) => format!("Editing template: {}", name),
            Message::NoTemplatesMatchingQuery(query) => format!("No templates matching '{}'", query),
            Message::TemplateSearchResults(query) => format!("Templates matching '{}':", query),
            Message::SelectTemplateAction => "What would you like to do?".to_string(),
            Message::PromptTemplateName => "Template name (unique identifier)".to_string(),
            Message::PromptTemplateTaskName => "Task name".to_string(),
            Message::PromptTemplateComment => "Comment (optional)".to_string(),
            Message::PromptTemplateCompleteness => "Default completeness (0-100)".to_string(),
            Message::CreatingTaskFromTemplate(name) => format!("Creating task from template '{}'", name),
            Message::SelectTemplate => "Select a template".to_string(),
            Message::CreateTemplateFirst => "Create templates with 'kasl template create'".to_string(),

            // === TAG MESSAGES ===
            Message::TagCreated(name) => format!("Tag '{}' created successfully.", name),
            Message::TagUpdated(name) => format!("Tag '{}' updated successfully.", name),
            Message::TagDeleted(name) => format!("Tag '{}' deleted successfully.", name),
            Message::TagNotFound(name) => format!("Tag '{}' not found.", name),
            Message::TagAlreadyExists(name) => format!("Tag '{}' already exists.", name),
            Message::NoTagsFound => "No tags found.".to_string(),
            Message::TagListHeader => "Tags:".to_string(),
            Message::EditingTag(name) => format!("Editing tag: {}", name),
            Message::SelectTagAction => "What would you like to do?".to_string(),
            Message::SelectTagToEdit => "Select tag to edit".to_string(),
            Message::SelectTagToDelete => "Select tag to delete".to_string(),
            Message::ConfirmDeleteTag(name) => format!("Delete tag '{}'?", name),
            Message::ConfirmDeleteTagWithTasks(name, count) => format!("Tag '{}' is used by {} task(s). Delete anyway?", name, count),
            Message::PromptTagName => "Tag name".to_string(),
            Message::PromptTagColor => "Tag color (e.g., blue, green, red)".to_string(),
            Message::NoTasksWithTag(tag) => format!("No tasks found with tag '{}'.", tag),
            Message::TasksWithTag(tag) => format!("Tasks with tag '{}':", tag),
            Message::TagsAddedToTask(tags) => format!("Tags added: {}", tags),

            // === SHORT INTERVALS MESSAGES ===
            Message::ShortIntervalsDetected(count, duration) => format!("Found {} short work intervals (total: {})", count, duration),
            Message::NoShortIntervalsFound(min) => format!("No work intervals shorter than {} minutes found.", min),
            Message::ShortIntervalsToRemove(count) => format!("Found {} short intervals to remove:", count),
            Message::RemovingPauses(count) => format!("Removing {} pauses to merge intervals...", count),
            Message::ShortIntervalsCleared(count) => format!("Successfully removed {} pauses and merged intervals.", count),
            Message::NoRemovablePausesFound => "No pauses found that can be removed to clear short intervals.".to_string(),
            Message::UpdatedReport => "Updated report:".to_string(),
            Message::PromptMinWorkInterval => "Minimum work interval (minutes)".to_string(),

            // === TIME ADJUSTMENT MESSAGES ===
            Message::SelectAdjustmentMode => "Select adjustment mode".to_string(),
            Message::PromptAdjustmentMinutes => "How many minutes to adjust?".to_string(),
            Message::PromptPauseStartTime => "When should the pause start? (HH:MM)".to_string(),
            Message::ConfirmTimeAdjustment => "Apply this time adjustment?".to_string(),
            Message::TimeAdjustmentApplied => "Time adjustment applied successfully.".to_string(),
            Message::AdjustmentPreview => "Time adjustment preview:".to_string(),
            Message::InvalidAdjustmentTooMuchTime => "Cannot adjust that much time - would result in invalid workday.".to_string(),
            Message::InvalidPauseOutsideWorkday => "Pause must be within workday hours.".to_string(),
            Message::WorkdayUpdateFailed => "Failed to update workday.".to_string(),

            // === PAUSE MESSAGES ===
            Message::PausesTitle(date) => format!("Pauses for {}", date),

            // === MONITOR MESSAGES ===
            Message::MonitorStarted {
                pause_threshold,
                poll_interval,
                activity_threshold,
            } => {
                format!(
                    "Monitor is running with pause threshold {}s, poll interval {}ms, activity threshold {}s",
                    pause_threshold, poll_interval, activity_threshold
                )
            }
            Message::MonitorStopped => "Monitor stopped".to_string(),
            Message::MonitorStartFailed => "Failed to start monitor".to_string(),
            Message::MonitorStopFailed => "Failed to stop monitor".to_string(),
            Message::MonitorExitedNormally => "Monitor exited normally".to_string(),
            Message::MonitorShuttingDown => "Shutting down monitor...".to_string(),
            Message::MonitorError(error) => format!("Monitor error: {}", error),
            Message::MonitorTaskPanicked(error) => format!("Monitor task panicked: {}", error),
            Message::PauseStarted => "Pause Start".to_string(),
            Message::PauseEnded => "Pause End".to_string(),

            // === WATCHER/DAEMON MESSAGES ===
            Message::WatcherStarted(pid) => format!("Watcher started in the background (PID: {}).", pid),
            Message::WatcherStopped(pid) => format!("Watcher process (PID: {}) stopped successfully.", pid),
            Message::WatcherStoppedSuccessfully => "Watcher stopped successfully".to_string(),
            Message::WatcherNotRunning => "Watcher is not running.".to_string(),
            Message::WatcherNotRunningPidNotFound => "Watcher does not appear to be running (PID file not found).".to_string(),
            Message::WatcherStartingForeground => "Starting watcher in foreground... Press Ctrl+C to exit.".to_string(),
            Message::WatcherStoppingExisting(pid) => format!("Stopping existing watcher (PID: {})...", pid),
            Message::WatcherFailedToStopExisting(error) => format!("Warning: Failed to stop existing daemon: {}", error),
            Message::WatcherFailedToStop(pid) => format!("Failed to stop watcher process (PID: {})", pid),
            Message::WatcherReceivedSigterm => "Received SIGTERM, shutting down gracefully...".to_string(),
            Message::WatcherReceivedSigint => "Received SIGINT, shutting down gracefully...".to_string(),
            Message::WatcherReceivedCtrlC => "Received Ctrl+C, shutting down gracefully...".to_string(),
            Message::WatcherCtrlCListenFailed(error) => format!("Failed to listen for Ctrl+C: {}", error),
            Message::WatcherSignalHandlingNotSupported => "Warning: Signal handling not supported on this platform".to_string(),
            Message::DaemonModeNotSupported => "Daemon mode is not supported on this platform.".to_string(),
            Message::FailedToGetCurrentExecutable => "Failed to get the path of the current executable".to_string(),
            Message::FailedToCreateSigtermHandler => "Failed to create SIGTERM handler".to_string(),
            Message::FailedToCreateSigintHandler => "Failed to create SIGINT handler".to_string(),

            // === UPDATE MESSAGES ===
            Message::UpdateAvailable { app_name, latest } => {
                format!(
                    "A new version of {} is available: v{}\nUpgrade now by running: {} update",
                    app_name, latest, app_name
                )
            }
            Message::UpdateCompleted { app_name, version } => {
                format!("The {} application has been successfully updated to version {}!", app_name, version)
            }
            Message::NoUpdateRequired => "No update required. You are using the latest version!".to_string(),
            Message::UpdateDownloadUrlNotSet => "Download URL not set".to_string(),
            Message::UpdateBinaryNotFoundInArchive => "Binary not found in the release archive.".to_string(),

            // === AUTHENTICATION MESSAGES ===
            Message::WrongPassword(count) => format!("You entered the wrong password {} times!", count),
            Message::InvalidCredentials => "Invalid credentials".to_string(),
            Message::SessionExpired => "Session expired".to_string(),
            Message::AuthenticationFailed(service) => format!("Authentication failed: {}", service),
            Message::JiraAuthenticateFailed => "Jira authenticate failed".to_string(),
            Message::LoginFailed => "Login failed".to_string(),
            Message::CredentialsNotSet => "Credentials not set!".to_string(),

            // === API MESSAGES ===
            Message::ApiConnectionFailed => "Failed to connect to API".to_string(),
            Message::ApiAuthFailed => "API authentication failed".to_string(),
            Message::ApiRequestFailed => "API request failed".to_string(),
            Message::GitlabFetchFailed(error) => format!("[kasl] Failed to get GitLab events: {}", error),
            Message::GitlabUserIdFailed(error) => format!("[kasl] Failed to get GitLab user ID: {}", error),
            Message::JiraFetchFailed(error) => format!("[kasl] Failed to get Jira issues: {}", error),
            Message::SiServerConfigNotFound => "SiServer configuration not found in config file.".to_string(),
            Message::SiServerSessionFailed(error) => format!("[kasl] Failed to get SiServer session for rest dates: {}", error),
            Message::SiServerRestDatesFailed(error) => format!("[kasl] Failed to request rest dates: {}", error),
            Message::SiServerRestDatesParsingFailed(error) => format!("[kasl] Failed to parse rest dates response: {}", error),

            // === DATABASE MESSAGES ===
            Message::DbConnectionFailed => "Failed to connect to database".to_string(),
            Message::DbQueryFailed => "Database query failed".to_string(),
            Message::DbMigrationFailed => "Database migration failed".to_string(),
            Message::NoIdSet => "No ID set".to_string(),

            // === FILE SYSTEM MESSAGES ===
            Message::FileNotFound => "File not found".to_string(),
            Message::FileReadError => "Failed to read file".to_string(),
            Message::FileWriteError => "Failed to write file".to_string(),
            Message::InvalidPidFileContent => "Invalid PID file content".to_string(),
            Message::DataStoragePathError => "DataStorage get_path error".to_string(),

            // === SYSTEM/PATH MESSAGES ===
            Message::PathQueryFailed(status) => format!("Failed to query PATH from registry: {:?}", status),
            Message::PathSetFailed => "Failed to set PATH in registry".to_string(),
            Message::FailedToJoinPaths => "Failed to join paths".to_string(),
            Message::FailedToExecuteRegQuery => "Failed to execute reg query".to_string(),
            Message::FailedToParseRegOutput => "Failed to parse reg query output".to_string(),
            Message::FailedToGetPathFromReg => "Failed to get PATH value from reg query".to_string(),
            Message::FailedToExecuteRegSet => "Failed to execute reg set".to_string(),
            Message::FailedToOpenProcess(code) => format!("Failed to open process: error code {}", code),
            Message::FailedToTerminateProcess(code) => format!("Failed to terminate process: error code {}", code),
            Message::ProcessNotFound => "Process doesn't exist".to_string(),
            Message::ProcessTerminationNotSupported => "Process termination not supported on this platform".to_string(),

            // === PRODUCTIVITY MESSAGES ===
            Message::MonthlyProductivity(percentage) => format!("Monthly work productivity: {:.1}%", percentage),

            // === ENCRYPTION/SECRET MESSAGES ===
            Message::EncryptionKeyMustBeSet => "ENCRYPTION_KEY must be set".to_string(),
            Message::EncryptionIvMustBeSet => "ENCRYPTION_IV must be set".to_string(),

            // === PROMPTS ===
            Message::PromptTaskName => "Enter task name".to_string(),
            Message::PromptTaskComment => "Enter comment".to_string(),
            Message::PromptTaskCompleteness => "Enter completeness".to_string(),
            Message::PromptGitlabToken => "Enter your GitLab private token".to_string(),
            Message::PromptGitlabUrl => "Enter the GitLab API URL".to_string(),
            Message::PromptJiraLogin => "Enter your Jira login".to_string(),
            Message::PromptJiraUrl => "Enter the Jira API URL".to_string(),
            Message::PromptJiraPassword => "Enter your Jira password".to_string(),
            Message::PromptSiLogin => "Enter your SiServer login".to_string(),
            Message::PromptSiAuthUrl => "Enter your SiServer login URL".to_string(),
            Message::PromptSiApiUrl => "Enter the SiServer API URL".to_string(),
            Message::PromptSiPassword => "Enter your SiServer password".to_string(),
            Message::PromptMinPauseDuration => "Enter minimum pause duration (minutes)".to_string(),
            Message::PromptPauseThreshold => "Enter pause threshold (seconds)".to_string(),
            Message::PromptPollInterval => "Enter poll interval (milliseconds)".to_string(),
            Message::PromptActivityThreshold => "Enter activity threshold (seconds)".to_string(),
            Message::PromptServerApiUrl => "Enter server API URL".to_string(),
            Message::PromptServerAuthToken => "Enter server auth token".to_string(),
            Message::PromptConfirmDelete => "Are you sure you want to delete this item?".to_string(),
            Message::PromptSelectOptions => "Select options".to_string(),
            Message::PromptSelectModules => "Select nodes to configure".to_string(),
            Message::PromptSelectTasks => "Select tasks".to_string(),
            Message::PromptSelectTasksToEdit => "Select tasks to edit".to_string(),

            // === GENERAL MESSAGES ===
            Message::OperationCompleted => "Operation completed successfully".to_string(),
            Message::OperationCancelled => "Operation cancelled".to_string(),
            Message::DataExported => "Data exported successfully".to_string(),
            Message::BackupCreated => "Backup created successfully".to_string(),
            Message::InvalidInput => "Invalid input provided".to_string(),
            Message::PermissionDenied => "Permission denied".to_string(),

            // === ERROR LOGGING ===
            Message::ErrorSendingEvents(error) => format!("[kasl] Error sending events: {}", error),
            Message::ErrorSendingMonthlyReport(error) => format!("[kasl] Error sending monthly report: {}", error),
            Message::ErrorInRdevListener(error) => format!("Error in rdev listener: {:?}", error),
            Message::ErrorRequestingRestDates(error) => format!("Error requesting rest dates: {}", error),

            // === SPECIFIC UI MESSAGES ===
            Message::SelectingTask(name) => format!("Selected task: {}", name),
            Message::SelectedTaskFormat(name, completeness) => format!("{} - {}%", name, completeness),

            // === MIGRATION MESSAGES ===
            Message::MigrationsFound(count) => format!("Found {} pending database migrations", count),
            Message::RunningMigration(version, name) => format!("Running migration v{}: {}", version, name),
            Message::MigrationCompleted(version) => format!("✓ Migration v{} completed", version),
            Message::MigrationFailed(version, error) => format!("✗ Migration v{} failed: {}", version, error),
            Message::AllMigrationsCompleted => "All database migrations completed successfully".to_string(),
            Message::DatabaseVersion(version) => format!("Current database version: {}", version),
            Message::DatabaseUpToDate => "Database schema is up to date".to_string(),
            Message::DatabaseNeedsUpdate => "Database schema needs to be updated".to_string(),
            Message::MigrationHistory => "Migration history:".to_string(),
            Message::NothingToRollback => "Nothing to rollback".to_string(),
            Message::RollingBack(from, to) => format!("Rolling back from v{} to v{}", from, to),
            Message::RollbackCompleted(version) => format!("Rollback to v{} completed", version),
        };

        write!(f, "{}", text)
    }
}
