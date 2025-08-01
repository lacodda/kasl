use super::types::Message;
use std::fmt::{Display, Formatter, Result};

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let text = match self {
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
        };

        write!(f, "{}", text)
    }
}
