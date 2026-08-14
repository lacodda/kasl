//! Message type definitions for the kasl application.
//!
//! Defines the central `Message` enum that represents all user-facing messages with type safety and compile-time verification.
//!
//! ## Usage
//!
//! ```rust
//! use kasl::libs::messages::types::Message;
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

/// Comprehensive message enumeration for all user-facing communication.
///
/// This enum represents every type of message that the kasl application can
/// present to users. It provides a type-safe way to handle all application
/// communication while ensuring consistent formatting and proper parameter
/// handling across all components.
///
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
    AutostartRequiresAdmin,
    AutostartCheckingAlternative,

    // === TASK MESSAGES ===
    TaskCreated,
    TaskUpdated,
    TaskNotFound,
    TaskUpdateFailed,
    TasksDeletedCount(usize),
    TasksNotFoundForDate(String),
    TasksNotFoundSad,
    TasksHeader,
    /// Summary line before unified discovery MultiSelect
    TasksDiscoverySummary {
        incomplete: usize,
        jira: usize,
        gitlab: usize,
    },
    /// Non-selectable separator between incomplete and other items
    TasksDiscoverySeparator,
    TasksDiscoverySearchingIncomplete,
    TasksDiscoveryFetchingExternal,
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
    WorkdayNotFoundForDate(String),
    WorkdayCreateFailed,
    WorkdayStarting(String),                    // date
    WorkdayCouldNotFindAfterFinalizing(String), // date

    // === CONFIGURATION MESSAGES ===
    ConfigSaved,
    ConfigDeleted,
    ConfigParseError,
    ConfigSaveError,
    ConfigModuleGitLab,
    ConfigModuleJira,
    ConfigModuleSiServer,
    ConfigModuleMonitor,
    ConfigModuleServer,
    ConfigModuleProductivity,
    ConfigModuleTaskDiscovery,
    ConfigModuleJiraInbox,
    TaskDiscoveryIgnoreListHeader,
    TaskDiscoveryIgnoreListEmpty,
    TaskDiscoveryIgnoreNameAdded(String),
    TaskDiscoveryIgnoreNameExists(String),
    TaskDiscoveryIgnoreNamesAdded(usize),

    // === REPORT MESSAGES ===
    DailyReportSent(String),   // date
    MonthlyReportSent(String), // date
    MonthlyReportTriggered,
    ReportSendFailed(String),        // status
    MonthlyReportSendFailed(String), // status
    ReportHeader(String),            // date
    WorkingHoursForMonth(String),    // month/year

    // === EXPORT MESSAGES ===
    ExportingData(String, String), // data type, format
    ExportCompleted(String),       // file path
    ExportingAllData,

    // === TEMPLATE MESSAGES ===
    TemplateCreated(String),
    TemplateUpdated(String),
    TemplateDeleted(String),
    TemplateNotFound(String),
    TemplateAlreadyExists(String),
    TemplateCreateFailed,
    NoTemplatesFound,
    TemplateListHeader,
    SelectTemplateToEdit,
    SelectTemplateToDelete,
    ConfirmDeleteTemplate(String),
    EditingTemplate(String),
    NoTemplatesMatchingQuery(String),
    TemplateSearchResults(String),
    SelectTemplateAction,
    PromptTemplateName,
    PromptTemplateTaskName,
    PromptTemplateComment,
    PromptTemplateCompleteness,
    CreatingTaskFromTemplate(String),
    SelectTemplate,
    CreateTemplateFirst,

    // === TAG MESSAGES ===
    TagCreated(String),
    TagUpdated(String),
    TagDeleted(String),
    TagNotFound(String),
    TagAlreadyExists(String),
    NoTagsFound,
    TagListHeader,
    EditingTag(String),
    SelectTagAction,
    SelectTagToEdit,
    SelectTagToDelete,
    ConfirmDeleteTag(String),
    ConfirmDeleteTagWithTasks(String, usize), // tag name, task count
    PromptTagName,
    PromptTagColor,
    NoTasksWithTag(String),
    TasksWithTag(String),
    TagsAddedToTask(String),

    // === SHORT INTERVALS MESSAGES ===
    ShortIntervalsDetected(usize, String), // count, total duration
    PromptMinWorkInterval,

    // === TIME ADJUSTMENT MESSAGES ===
    WorkdayUpdateFailed,

    // === PAUSE MESSAGES ===
    PausesTitle(String),

    // === MONITOR MESSAGES ===
    MonitorStarted {
        pause_threshold: u64,
        poll_interval: u64,
        activity_threshold: u64,
    },
    MonitorExitedNormally,
    MonitorShuttingDown,
    MonitorError(String),
    MonitorTaskPanicked(String),
    PauseStarted,
    PauseEnded,

    // === WATCHER/DAEMON MESSAGES ===
    WatcherStarted(u32), // PID
    WatcherStopped(u32), // PID
    WatcherNotRunning,
    WatcherNotRunningPidNotFound,
    WatcherStartingForeground,
    WatcherStoppingExisting(String),     // PID
    WatcherFailedToStopExisting(String), // error
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
    UpdateLatestTagNotFound(String),
    WatcherStoppingForUpdate,
    WatcherRestartingAfterUpdate,
    WatcherStoppingForConfig,
    WatcherRestartingAfterConfig,
    WatcherRestarted,
    WatcherRestartFailed {
        error: String,
    },
    UpdateBinaryNotFoundInArchive,

    // === AUTHENTICATION MESSAGES ===
    WrongPassword(i32), // attempt count

    // === API MESSAGES ===
    GitlabFetchFailed(String),  // error message
    GitlabUserIdFailed(String), // error message
    JiraFetchFailed(String),    // error message
    SiServerConfigNotFound,
    SiServerSessionFailed(String),          // error message
    SiServerRestDatesFailed(String),        // error message
    SiServerRestDatesParsingFailed(String), // error message

    // === DATABASE MESSAGES ===
    DatabaseOperationFailed {
        operation: String,
        error: String,
    },
    NoIdSet,

    // === FILE SYSTEM MESSAGES ===
    FileNotFound,
    InvalidPidFileContent,

    // === SYSTEM/PATH MESSAGES ===
    PathConfigured,
    PathConfigWarning {
        error: String,
    },
    PathRegistryQueryError {
        status: String,
    },
    PathRegistryUpdateError {
        status: String,
        stderr: String,
    },
    FailedToJoinPaths,
    FailedToExecuteRegQuery,
    FailedToParseRegOutput,
    FailedToGetPathFromReg,
    FailedToExecuteRegSet,
    FailedToOpenProcess(u32),      // error code
    FailedToTerminateProcess(u32), // error code
    ProcessTerminationNotSupported,

    // === PRODUCTIVITY MESSAGES ===
    MonthlyProductivity(f64), // percentage
    LowProductivityWarning {
        current: f64,
        threshold: f64,
    },
    ProductivityTooLowToSend {
        current: f64,
        threshold: f64,
    },
    ManualPauseCreated {
        start_time: String,
        end_time: String,
        duration_minutes: u64,
    },
    ManualPauseOverlaps {
        start_time: String,
        end_time: String,
    },
    ManualPauseRemoved(i32),
    ManualPauseNotFound(i32),

    // === ENCRYPTION/SECRET MESSAGES ===

    // === PROMPTS ===
    PromptTaskName,
    TaskNameMergedFromPaste,
    PromptTaskComment,
    PromptTaskCompleteness,
    PromptMinPauseDuration,
    PromptPauseThreshold,
    PromptPollInterval,
    PromptActivityThreshold,
    PromptMinProductivityThreshold,
    PromptWorkdayHours,
    PromptMinWorkdayFraction,
    PromptServerApiUrl,
    PromptServerAuthToken,
    PromptSelectModules,
    PromptSelectTasksToImport,
    PromptSelectTasksToIgnore,
    PromptSelectIgnoreNamesToRemove,
    PromptAddIgnoreName,

    // === GENERAL MESSAGES ===
    OperationCompleted,
    OperationCancelled,
    InvalidInput,
    PermissionDenied,
    DeprecatedCommand(&'static str, &'static str), // old name, new name

    // === ERROR LOGGING ===
    ErrorSendingEvents(String),        // error message
    ErrorSendingMonthlyReport(String), // error message
    ErrorInRdevListener(String),       // error message
    ErrorRequestingRestDates(String),  // error message

    // === SPECIFIC UI MESSAGES ===
    SelectingTask(String), // task name

    // === JIRA INBOX MESSAGES ===
    JiraInboxRequiresJiraConfig,
    JiraInboxEmpty,
    JiraInboxListHeader,
    JiraInboxSynced {
        fetched: usize,
        new_count: usize,
        changed: usize,
        gone: usize,
    },
    JiraInboxNewIssues(usize),
    JiraInboxNotFound(String),
    JiraInboxPinned(String),
    JiraInboxUnpinned(String),
    JiraInboxDismissed(String),
    JiraInboxOpened(String),
    JiraInboxTaken(String),
    JiraInboxOpenFailed(String),
    PromptJiraInboxEnabled,
    PromptJiraInboxPollInterval,
    PromptJiraInboxNotify,
    PromptJiraInboxSortFieldId,
    PromptJiraInboxSortFieldLabel,
    PromptJiraInboxExtraFieldId,
    PromptJiraInboxExtraFieldLabel,

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
