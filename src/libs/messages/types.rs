#[derive(Debug, Clone)]
pub enum Message {
    // === AUTOSTART MESSAGES ===
    AutostartEnabled,
    AutostartEnabledUser,
    AutostartDisabled,
    AutostartAlreadyDisabled,
    AutostartEnableFailed(String),
    AutostartDisableFailed(String),
    AutostartStatus(String),
    AutostartNotImplemented,
    AutostartRequiresAdmin,
    AutostartCheckingAlternative,

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
    TasksHeader,
    TasksIncompleteHeader,
    TasksGitlabHeader,
    TasksJiraHeader,
    NoTaskIdsProvided,
    TasksNotFoundForIds(Vec<i32>),
    TasksToBeDeleted,
    ConfirmDeleteTask,
    ConfirmDeleteTasks(usize),
    ConfirmDeleteAllTodayTasks(usize),
    ConfirmDeleteAllTodayTasksFinal,
    NoTasksForToday,
    TaskNotFoundWithId(i32),
    CurrentTaskState,
    TaskEditPreview,
    ConfirmTaskUpdate,
    NoChangesDetected,
    NoTasksSelected,
    SelectTasksToEdit,
    EditingTask(String),
    TaskUpdatedWithName(String),
    TaskSkippedNoChanges(String),
    TaskEditingCompleted,
    PromptTaskNameEdit,
    PromptTaskCommentEdit,
    PromptTaskCompletenessEdit,
    TaskCompletenessRange,

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
    ConfigModuleGitLab,
    ConfigModuleJira,
    ConfigModuleSiServer,
    ConfigModuleMonitor,
    ConfigModuleServer,

    // === REPORT MESSAGES ===
    DailyReportSent(String),   // date
    MonthlyReportSent(String), // date
    MonthlyReportTriggered,
    ReportSendFailed(String),        // status
    MonthlyReportSendFailed(String), // status
    ReportHeader(String),            // date
    WorkingHoursForMonth(String),    // month/year

    // === SHORT INTERVALS MESSAGES ===
    ShortIntervalsDetected(usize, String), // count, total duration
    NoShortIntervalsFound(u64),            // min_minutes
    UseReportClearCommand,
    ShortIntervalsToRemove(usize), // count
    RemovingPauses(usize),         // count
    ShortIntervalsCleared(usize),  // deleted count
    NoRemovablePausesFound,
    UpdatedReport,
    PromptMinWorkInterval,

    // === TIME ADJUSTMENT MESSAGES ===
    SelectAdjustmentMode,
    PromptAdjustmentMinutes,
    PromptPauseStartTime,
    ConfirmTimeAdjustment,
    TimeAdjustmentApplied,
    AdjustmentPreview,
    InvalidAdjustmentTooMuchTime,
    InvalidPauseOutsideWorkday,
    WorkdayUpdateFailed,

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
    MonitorExitedNormally,
    MonitorShuttingDown,
    MonitorError(String),
    MonitorTaskPanicked(String),
    PauseStarted,
    PauseEnded,

    // === WATCHER/DAEMON MESSAGES ===
    WatcherStarted(u32), // PID
    WatcherStopped(u32), // PID
    WatcherStoppedSuccessfully,
    WatcherNotRunning,
    WatcherNotRunningPidNotFound,
    WatcherStartingForeground,
    WatcherStoppingExisting(String),     // PID
    WatcherFailedToStopExisting(String), // error
    WatcherFailedToStop(u32),            // PID
    WatcherReceivedSigterm,
    WatcherReceivedSigint,
    WatcherReceivedCtrlC,
    WatcherCtrlCListenFailed(String), // error
    WatcherSignalHandlingNotSupported,
    DaemonModeNotSupported,
    FailedToGetCurrentExecutable,
    FailedToCreateSigtermHandler,
    FailedToCreateSigintHandler,

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
    UpdateDownloadUrlNotSet,
    UpdateBinaryNotFoundInArchive,

    // === AUTHENTICATION MESSAGES ===
    WrongPassword(i32), // attempt count
    InvalidCredentials,
    SessionExpired,
    AuthenticationFailed(String), // service name
    JiraAuthenticateFailed,
    LoginFailed,
    CredentialsNotSet,

    // === API MESSAGES ===
    ApiConnectionFailed,
    ApiAuthFailed,
    ApiRequestFailed,
    GitlabFetchFailed(String),  // error message
    GitlabUserIdFailed(String), // error message
    JiraFetchFailed(String),    // error message
    SiServerConfigNotFound,
    SiServerSessionFailed(String),          // error message
    SiServerRestDatesFailed(String),        // error message
    SiServerRestDatesParsingFailed(String), // error message

    // === DATABASE MESSAGES ===
    DbConnectionFailed,
    DbQueryFailed,
    DbMigrationFailed,
    NoIdSet,

    // === FILE SYSTEM MESSAGES ===
    FileNotFound,
    FileReadError,
    FileWriteError,
    InvalidPidFileContent,
    DataStoragePathError,

    // === SYSTEM/PATH MESSAGES ===
    PathQueryFailed(String), // status
    PathSetFailed,
    FailedToJoinPaths,
    FailedToExecuteRegQuery,
    FailedToParseRegOutput,
    FailedToGetPathFromReg,
    FailedToExecuteRegSet,
    FailedToOpenProcess(u32),      // error code
    FailedToTerminateProcess(u32), // error code
    ProcessNotFound,
    ProcessTerminationNotSupported,

    // === PRODUCTIVITY MESSAGES ===
    MonthlyProductivity(f64), // percentage

    // === ENCRYPTION/SECRET MESSAGES ===
    EncryptionKeyMustBeSet,
    EncryptionIvMustBeSet,

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

    // === MIGRATION MESSAGES ===
    MigrationsFound(usize),        // count
    RunningMigration(u32, String), // version, name
    MigrationCompleted(u32),       // version
    MigrationFailed(u32, String),  // version, error
    AllMigrationsCompleted,
    DatabaseVersion(u32),
    DatabaseUpToDate,
    DatabaseNeedsUpdate,
    MigrationHistory,
    NothingToRollback,
    RollingBack(u32, u32),  // from, to
    RollbackCompleted(u32), // version
}
