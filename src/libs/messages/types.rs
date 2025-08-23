//! Message type definitions for the kasl application.
//!
//! Defines the central `Message` enum that represents all user-facing messages with type safety and compile-time verification.
//!
//! ## Features
//!
//! - **Type Safety**: All messages are strongly typed with appropriate parameters
//! - **Centralization**: Single enum captures all application messaging needs  
//! - **Extensibility**: Easy addition of new message types and categories
//! - **Organization**: Logical grouping by functional categories
//! - **Internationalization**: Structure supports future localization efforts
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
/// ## Message Categories
///
/// The enum is organized into logical categories that correspond to different
/// areas of application functionality. Each category groups related messages
/// to improve maintainability and make it easier to understand the scope
/// of each functional area.
///
/// ## Parameter Conventions
///
/// - **String Parameters**: Used for names, descriptions, and user-provided text
/// - **Numeric Parameters**: Used for counts, IDs, and measurements
/// - **Status Parameters**: Used for system states and operation results
/// - **Structured Parameters**: Named fields for complex message data
///
/// ## Adding New Messages
///
/// When adding new message variants:
/// 1. Choose the appropriate category section
/// 2. Use descriptive names that clearly indicate the message purpose
/// 3. Include necessary parameters with appropriate types
/// 4. Add corresponding text in the `Display` implementation
/// 5. Document the message purpose and usage context
///
/// ## Backward Compatibility
///
/// The enum is designed to maintain backward compatibility:
/// - New variants can be added without breaking existing code
/// - Parameter changes should be additive when possible
/// - Deprecated messages can be maintained for transition periods
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
    ConfigDeleted,
    ConfigLoaded,
    ConfigFileNotFound,
    ConfigParseError,
    ConfigSaveError,
    ConfigModuleGitLab,
    ConfigModuleJira,
    ConfigModuleSiServer,
    ConfigModuleMonitor,
    ConfigModuleServer,
    ConfigModuleProductivity,

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
    ExportFailed(String), // error

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
    NoShortIntervalsFound(u64),            // min_minutes
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
    WatcherStoppingForUpdate,
    WatcherRestartingAfterUpdate,
    WatcherStoppingForConfig,
    WatcherRestartingAfterConfig,
    WatcherRestarted,
    WatcherRestartFailed { error: String },
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
    DatabaseOperationFailed { operation: String, error: String },
    NoIdSet,

    // === FILE SYSTEM MESSAGES ===
    FileNotFound,
    FileReadError,
    FileWriteError,
    InvalidPidFileContent,
    DataStoragePathError,

    // === SYSTEM/PATH MESSAGES ===
    PathConfigured,
    PathConfigWarning { error: String },
    PathQueryFailed(String), // status
    PathSetFailed,
    PathRegistryQueryError { status: String },
    PathRegistryUpdateError { status: String, stderr: String },
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
    LowProductivityWarning {
        current: f64,
        threshold: f64,
        needed_break_minutes: u64,
    },
    ProductivityTooLowToSend {
        current: f64,
        threshold: f64,
        needed_break_minutes: u64,
    },
    BreakCreated {
        start_time: String,
        end_time: String,
        duration_minutes: u64,
    },
    BreakCreateFailed(String),
    BreakSuggestionCommand {
        auto_minutes: u64,
    },
    BreakInteractivePrompt,
    BreakDurationPrompt {
        min_duration: u64,
        max_duration: u64,
    },
    BreakPlacementOptions,
    BreakOptionSelected(usize),
    BreakConflictsWithPauses,
    NoValidBreakPlacement,
    ProductivityRecalculated(f64),

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
    PromptMinProductivityThreshold,
    PromptWorkdayHours,
    PromptMinWorkdayFraction,
    PromptMinBreakDuration,
    PromptMaxBreakDuration,
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
