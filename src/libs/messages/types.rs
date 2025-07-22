#[derive(Debug, Clone)]
pub enum Message {
    // === TASK MESSAGES ===
    TaskCreated,
    TaskUpdated,
    TaskDeleted,
    TaskNotFound,
    TaskCreateFailed,
    TaskUpdateFailed,
    TaskDeleteFailed,
    TasksDeletedCount(usize),
    TasksNotFoundForDate(String),
    TasksNotFoundSad, // "Tasks not found(("

    // === WORKDAY MESSAGES ===
    WorkdayEnded,
    WorkdayNotFound,
    WorkdayNotFoundForDate(String),
    WorkdayCreateFailed,
    WorkdayStarting(String),                    // date
    WorkdayCouldNotFindAfterFinalizing(String), // date

    // === CONFIGURATION MESSAGES ===
    ConfigSaved,
    ConfigLoaded,
    ConfigFileNotFound,
    ConfigParseError,
    ConfigSaveError,

    // === REPORT MESSAGES ===
    DailyReportSent(String),   // date
    MonthlyReportSent(String), // date
    MonthlyReportTriggered,
    ReportSendFailed(String),        // status
    MonthlyReportSendFailed(String), // status

    // === PAUSE MESSAGES ===
    PausesTitle(String),

    // === MONITOR MESSAGES ===
    MonitorStarted {
        pause_threshold: u64,
        poll_interval: u64,
        activity_threshold: u64,
    },
    MonitorStopped,
    MonitorStartFailed,
    MonitorStopFailed,
    PauseStarted,
    PauseEnded,

    // === WATCHER/DAEMON MESSAGES ===
    WatcherStarted(u32), // PID
    WatcherStopped(u32), // PID
    WatcherStoppedSuccessfully,
    WatcherNotRunning,
    WatcherStartingForeground,

    // === UPDATE MESSAGES ===
    UpdateAvailable {
        app_name: String,
        latest: String,
    },
    UpdateCompleted {
        app_name: String,
        version: String,
    },
    NoUpdateRequired,

    // === AUTHENTICATION MESSAGES ===
    WrongPassword(i32), // attempt count
    InvalidCredentials,
    SessionExpired,
    AuthenticationFailed(String), // service name

    // === API MESSAGES ===
    ApiConnectionFailed,
    ApiAuthFailed,
    ApiRequestFailed,
    GitlabFetchFailed(String), // error message
    JiraFetchFailed(String),   // error message
    SiServerConfigNotFound,

    // === DATABASE MESSAGES ===
    DbConnectionFailed,
    DbQueryFailed,
    DbMigrationFailed,

    // === FILE SYSTEM MESSAGES ===
    FileNotFound,
    FileReadError,
    FileWriteError,

    // === PROMPTS ===
    PromptTaskName,
    PromptTaskComment,
    PromptTaskCompleteness,
    PromptGitlabToken,
    PromptGitlabUrl,
    PromptJiraLogin,
    PromptJiraUrl,
    PromptJiraPassword,
    PromptSiLogin,
    PromptSiAuthUrl,
    PromptSiApiUrl,
    PromptSiPassword,
    PromptMinPauseDuration,
    PromptPauseThreshold,
    PromptPollInterval,
    PromptActivityThreshold,
    PromptServerApiUrl,
    PromptServerAuthToken,
    PromptConfirmDelete,
    PromptSelectOptions,
    PromptSelectModules,
    PromptSelectTasks,
    PromptSelectTasksToEdit,

    // === GENERAL MESSAGES ===
    OperationCompleted,
    OperationCancelled,
    DataExported,
    BackupCreated,
    InvalidInput,
    PermissionDenied,

    // === ERROR LOGGING ===
    ErrorSendingEvents(String),        // error message
    ErrorSendingMonthlyReport(String), // error message
    ErrorInRdevListener(String),       // error message
    ErrorRequestingRestDates(String),  // error message

    // === SPECIFIC UI MESSAGES ===
    SelectingTask(String),           // task name
    SelectedTaskFormat(String, i32), // task name, completeness
}
