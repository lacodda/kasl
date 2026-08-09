//! Configuration management system for the kasl application.
//!
//! Provides a comprehensive configuration management system that handles application
//! settings, external API integrations, and activity monitoring parameters.
//!
//! ## Features
//!
//! - **Multi-Service Integration**: Manages configurations for Jira, GitLab, and custom APIs
//! - **Activity Monitoring**: Configures behavior for work time tracking and pause detection
//! - **Interactive Setup**: Provides guided configuration wizards for all modules
//! - **Cross-Platform Persistence**: Handles configuration storage across Windows, macOS, and Linux
//! - **System Integration**: Manages global PATH configuration for CLI availability
//!
//! ## Usage
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use kasl::libs::config::Config;
//!
//! let config = Config::read()?;
//! let updated_config = Config::init()?;
//! updated_config.save()?;
//! # Ok(())
//! # }
//! ```

use super::data_storage::DataStorage;
use crate::api::gitlab::GitLabConfig;
use crate::api::jira::JiraConfig;
use crate::api::si::SiConfig;
use crate::libs::messages::Message;
use crate::libs::task::normalize_task_name;
use crate::{msg_error, msg_info, msg_print, msg_success};
use anyhow::Result;
use dialoguer::{Confirm, Input, MultiSelect, theme::ColorfulTheme};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::str;

/// Configuration file name used for storing application settings.
///
/// This constant ensures consistency across the application when referencing
/// the main configuration file. The file is stored in platform-specific
/// application data directories.
pub const CONFIG_FILE_NAME: &str = "config.json";

/// Represents a configurable module in the application.
///
/// This structure is used during interactive configuration setup to display
/// available modules and allow users to select which integrations they want
/// to configure. Each module has a unique key for internal identification
/// and a human-readable name for display purposes.
#[derive(Debug, Clone)]
pub struct ConfigModule {
    /// Unique identifier for the module used in configuration routing
    pub key: String,
    /// Display name shown to users during interactive setup
    pub name: String,
}

/// Activity monitor configuration settings.
///
/// Controls the behavior of the background activity monitoring system that tracks
/// user presence, detects work patterns, and manages pause recording.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MonitorConfig {
    /// Minimum pause duration in minutes to be recorded in the database.
    ///
    /// Pauses shorter than this threshold are considered brief interruptions
    /// (like answering a quick question) rather than actual breaks and are
    /// not stored as separate pause records. This helps keep pause data
    /// meaningful and reduces database noise.
    pub min_pause_duration: u64,

    /// Inactivity threshold in seconds before a pause is detected.
    ///
    /// When user input (keyboard, mouse) is not detected for this duration,
    /// the monitor considers the user to be on a pause. This value should
    /// be long enough to avoid false positives during normal work (like
    /// reading or thinking) but short enough to capture actual breaks.
    pub pause_threshold: u64,

    /// Poll interval in milliseconds for checking activity status.
    ///
    /// This determines how frequently the monitor checks whether the user
    /// has been inactive long enough to trigger pause detection. Lower
    /// values provide more responsive detection but increase CPU usage.
    /// Values between 500-1000ms provide good balance of responsiveness
    /// and performance.
    pub poll_interval: u64,

    /// Activity duration threshold in seconds for workday start detection.
    ///
    /// Continuous activity must exceed this duration before the system
    /// considers a workday to have truly started. This prevents brief
    /// interactions (like checking time or messages) from incorrectly
    /// starting work time tracking, especially during off-hours.
    pub activity_threshold: u64,

    /// Minimum work interval in minutes for interval merging.
    ///
    /// Work intervals shorter than this duration are merged with adjacent
    /// intervals to create more meaningful work blocks. This reduces
    /// fragmentation in work time reports caused by very brief pauses
    /// and helps present cleaner time tracking data.
    pub min_work_interval: u64,

    /// Maximum gap in seconds between two consecutive pauses to merge them.
    ///
    /// When the activity monitor briefly registers a stray input between two
    /// otherwise continuous inactivity periods, it splits a single break into
    /// several adjacent pause records. Pauses separated by a gap no longer than
    /// this value are treated as one continuous pause so that sub-threshold
    /// segments are not dropped from calculations. The value should stay small
    /// (a few tens of seconds) so that genuine short work periods between pauses
    /// are preserved rather than swallowed into the break.
    #[serde(default = "default_pause_merge_gap")]
    pub pause_merge_gap: u64,
}

/// Default gap (in seconds) below which consecutive pauses are merged.
///
/// Used both by [`MonitorConfig::default`] and by serde when an existing
/// configuration file predates the `pause_merge_gap` field.
fn default_pause_merge_gap() -> u64 {
    30
}

/// Productivity management configuration settings.
///
/// Controls the behavior of productivity monitoring, break recommendations,
/// and report validation based on productivity thresholds.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProductivityConfig {
    /// Minimum acceptable productivity percentage.
    ///
    /// Productivity below this threshold triggers warnings in reports
    /// and prevents report submission until breaks are added to reach
    /// the minimum level. Should be set between 70-85% for most users.
    pub min_productivity_threshold: f64,

    /// Expected workday duration in hours.
    ///
    /// Used for calculating break recommendations and determining when
    /// enough of the workday has passed to suggest productivity improvements.
    /// Typical values are 7.5-8.5 hours for standard workdays.
    pub workday_hours: f64,

    /// Fraction of workday that must pass before suggesting breaks.
    ///
    /// Productivity warnings and break suggestions are only shown after
    /// this portion of the expected workday has elapsed. This prevents
    /// premature suggestions during normal workday startup.
    /// Typical value: 0.5 (50% of workday)
    pub min_workday_fraction_before_suggest: f64,
}

/// Daily report export configuration.
///
/// Controls where generated report files are stored by default and how their
/// file names are constructed when an explicit `--output` path is not provided.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ReportConfig {
    /// Directory where generated report files are saved by default.
    ///
    /// When set, report exports without an explicit `--output` path are written
    /// into this directory (created automatically if missing). When unset, the
    /// legacy behavior is used (a timestamped file in the current directory).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,

    /// File name template (without extension) for generated reports.
    ///
    /// Supported placeholders:
    /// - `{date}` — the report date in `YYYY-MM-DD` format
    /// - `{seq}`  — a per-day sequence suffix: empty for the first report of the
    ///   day, then `_2`, `_3`, … for subsequent reports on the same date
    ///
    /// The file extension is appended automatically based on the export format.
    /// Defaults to `daily_report_{date}{seq}` when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename_template: Option<String>,

    /// Language code for localizing report labels, month and weekday names.
    ///
    /// Supported built-in values: `ru` (default) and `en`. Unknown or unset
    /// values fall back to `ru`, preserving the original report wording.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Name of the design template controlling the report's visual style.
    ///
    /// Corresponds to a file `<data>/report_templates/<name>.json`. When unset
    /// or missing on disk, the built-in `siserver` template is used. The
    /// `siserver` template reproduces the original SiServer look.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

/// External server configuration for report submission.
///
/// This structure contains the connection parameters for external reporting
/// systems that can receive work time reports and task summaries. It supports
/// custom company APIs or third-party time tracking services that accept
/// HTTP-based report submissions.
///
/// ## Security Considerations
///
/// - API URLs should use HTTPS in production environments
/// - Auth tokens are stored in plain text in configuration files
/// - Consider using environment variables for sensitive tokens in production
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServerConfig {
    /// Base URL of the external reporting API server.
    ///
    /// This should be the root URL of the API endpoint that accepts
    /// report submissions. The actual report endpoints will be constructed
    /// by appending specific paths to this base URL.
    ///
    /// Example: `https://api.company.com/timetracking`
    pub api_url: String,

    /// Authentication token for API access.
    ///
    /// This token is included in HTTP headers when submitting reports
    /// to authenticate the client with the external server. The token
    /// format and authentication scheme depend on the target API's
    /// requirements (Bearer tokens, API keys, etc.).
    pub auth_token: String,
}

/// Main configuration container for the entire application.
///
/// This structure serves as the root configuration object that encompasses
/// all service integrations and system settings. Each field represents an
/// optional module that can be configured independently, allowing users to
/// enable only the integrations they need.
///
/// ## Optional Configuration Pattern
///
/// All service configurations are optional (`Option<T>`), which provides
/// several benefits:
/// - Users can configure only the services they use
/// - Missing configurations don't break the application
/// - New integrations can be added without breaking existing setups
/// - Configuration files remain clean and focused
///
/// ## Serialization Behavior
///
/// The `skip_serializing_if = "Option::is_none"` attribute ensures that
/// unconfigured services are omitted from the JSON output, keeping
/// configuration files clean and readable.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    /// SearchInform internal API configuration.
    ///
    /// When configured, enables integration with company-specific APIs
    /// for advanced reporting and task management features.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub si: Option<SiConfig>,

    /// GitLab API integration configuration.
    ///
    /// Enables automatic discovery of commits and merge requests
    /// for task creation and progress tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gitlab: Option<GitLabConfig>,

    /// Jira API integration configuration.
    ///
    /// Provides access to issue tracking for automatic task import
    /// and work item synchronization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jira: Option<JiraConfig>,

    /// Activity monitoring configuration.
    ///
    /// Controls the behavior of the background process that tracks
    /// user activity and manages work time detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<MonitorConfig>,

    /// External reporting server configuration.
    ///
    /// Enables submission of reports to external time tracking
    /// or project management systems.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerConfig>,

    /// Productivity management configuration.
    ///
    /// Controls productivity thresholds, break recommendations,
    /// and report validation based on productivity metrics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub productivity: Option<ProductivityConfig>,

    /// Daily report export configuration.
    ///
    /// Controls the default output directory and file name template used when
    /// exporting reports without an explicit output path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<ReportConfig>,

    /// Task discovery settings for `kasl task --find`.
    ///
    /// Holds the ignore list used to filter out noisy commits and tasks.
    /// When absent, built-in defaults are applied at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_discovery: Option<TaskDiscoveryConfig>,

    /// Background Jira inbox polling and toast notifications.
    ///
    /// Requires `jira` to be configured. When absent, inbox polling is disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira_inbox: Option<JiraInboxConfig>,
}

/// Settings for polling assigned open Jira issues into the local inbox.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct JiraInboxConfig {
    /// Whether the watcher should poll Jira for open assigned issues.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Seconds between Jira inbox polls (default 300 = 5 minutes).
    #[serde(default = "default_jira_inbox_poll_interval")]
    pub poll_interval_secs: u64,

    /// Whether to show a desktop toast when a new issue appears.
    #[serde(default = "default_true")]
    pub notify: bool,

    /// Extra Jira fields to fetch (custom fields such as Scoring).
    #[serde(default)]
    pub custom_fields: Vec<JiraCustomField>,

    /// Field id used for ranking (DESC), typically Scoring (`customfield_…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_by_field: Option<String>,
}

/// A user-configured Jira custom field for inbox sync / display.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct JiraCustomField {
    /// Jira field id, e.g. `customfield_12345`.
    pub id: String,
    /// Human-readable label, e.g. `Scoring`.
    pub label: String,
}

fn default_true() -> bool {
    true
}

fn default_jira_inbox_poll_interval() -> u64 {
    300
}

impl Default for JiraInboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_interval_secs: default_jira_inbox_poll_interval(),
            notify: true,
            custom_fields: Vec::new(),
            sort_by_field: None,
        }
    }
}

impl JiraInboxConfig {
    /// Field ids to request from Jira search (custom fields only).
    pub fn extra_field_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.custom_fields.iter().map(|f| f.id.trim().to_string()).filter(|id| !id.is_empty()).collect();
        if let Some(sort_id) = &self.sort_by_field {
            let trimmed = sort_id.trim();
            if !trimmed.is_empty() && !ids.iter().any(|id| id == trimmed) {
                ids.push(trimmed.to_string());
            }
        }
        ids
    }
}

/// Settings for intelligent task discovery (`kasl task --find`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TaskDiscoveryConfig {
    /// Names/prefixes of tasks and commits to exclude from discovery.
    ///
    /// Matching is case-insensitive after normalization; a pattern matches
    /// when the candidate name equals it or starts with it.
    #[serde(default = "default_ignore_names")]
    pub ignore_names: Vec<String>,
}

/// Built-in ignore patterns moved from the former hardcoded noise filter.
pub fn default_ignore_names() -> Vec<String> {
    vec![
        "Merge remote-tracking branch".to_string(),
        "Merge branch ".to_string(),
        "update webui".to_string(),
    ]
}

impl Default for TaskDiscoveryConfig {
    fn default() -> Self {
        Self {
            ignore_names: default_ignore_names(),
        }
    }
}

impl Default for MonitorConfig {
    /// Provides sensible defaults for monitor configuration.
    ///
    /// These default values are carefully chosen based on typical work patterns
    /// and provide a good balance between accurate detection and minimal false
    /// positives. Users can adjust these values through the configuration system
    /// to match their specific work environment and preferences.
    ///
    /// ## Default Values Rationale
    ///
    /// - **20 minutes minimum pause**: Captures meaningful breaks while ignoring brief interruptions
    /// - **60 seconds inactivity threshold**: Allows for reading/thinking time while detecting real pauses
    /// - **500ms polling interval**: Provides responsive detection with reasonable resource usage
    /// - **30 seconds activity threshold**: Prevents false workday starts from brief interactions
    /// - **10 minutes minimum work interval**: Reduces fragmentation in work reports
    ///
    /// Default values:
    /// - 20 minutes minimum pause duration
    /// - 60 seconds inactivity threshold
    /// - 500ms polling interval
    /// - 30 seconds activity threshold
    /// - 10 minutes minimum work interval
    fn default() -> Self {
        MonitorConfig {
            min_pause_duration: 20,
            pause_threshold: 60,
            poll_interval: 500,
            activity_threshold: 30,
            min_work_interval: 10,
            pause_merge_gap: default_pause_merge_gap(),
        }
    }
}

impl Default for ProductivityConfig {
    /// Provides sensible defaults for productivity configuration.
    ///
    /// These defaults are based on common productivity management practices
    /// and provide a good starting point for most users. Values can be
    /// adjusted through configuration to match individual work styles.
    ///
    /// ## Default Values Rationale
    ///
    /// - **75% minimum productivity**: Reasonable threshold allowing for breaks
    /// - **8 hours workday**: Standard full-time work expectation
    /// - **50% workday fraction**: Wait until mid-day before warning on productivity
    fn default() -> Self {
        ProductivityConfig {
            min_productivity_threshold: 75.0,
            workday_hours: 8.0,
            min_workday_fraction_before_suggest: 0.5,
        }
    }
}

impl Default for Config {
    /// Creates a default configuration with all modules disabled.
    ///
    /// This provides a clean starting point for new installations where
    /// users can selectively enable and configure only the services they need.
    /// All optional configurations are set to `None`, requiring explicit
    /// setup through the interactive configuration system or manual editing.
    fn default() -> Self {
        Config {
            si: None,
            gitlab: None,
            jira: None,
            monitor: None,
            server: None,
            productivity: None,
            report: None,
            task_discovery: None,
            jira_inbox: None,
        }
    }
}

impl Config {
    /// Reads configuration from the filesystem.
    ///
    /// This method attempts to load the configuration file from the platform-specific
    /// application data directory. If no configuration file exists, it returns a
    /// default configuration with all modules disabled, allowing the application
    /// to function with minimal setup.
    ///
    /// ## File Location
    ///
    /// The configuration file location varies by platform:
    /// - **Windows**: `%LOCALAPPDATA%\lacodda\kasl\config.json`
    /// - **macOS**: `~/Library/Application Support/lacodda/kasl/config.json`
    /// - **Linux**: `~/.local/share/lacodda/kasl/config.json`
    ///
    /// ## Error Handling
    ///
    /// - **Missing file**: Returns default configuration (not an error)
    /// - **Corrupted file**: Returns parsing error
    /// - **Permission issues**: Returns filesystem error
    ///
    /// # Returns
    ///
    /// Returns the loaded configuration or a default configuration if no file exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration file exists but cannot be read or parsed.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::libs::config::Config;
    ///
    /// // Load configuration, falling back to defaults if no file exists
    /// let config = Config::read()?;
    ///
    /// // Check if Jira is configured
    /// if config.jira.is_some() {
    ///     println!("Jira integration is configured");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn read() -> Result<Config> {
        // Resolve the configuration file path using the data storage system
        let config_file_path = DataStorage::new().get_path(CONFIG_FILE_NAME)?;

        // If no configuration file exists, return default configuration
        // This allows the application to run with minimal setup
        if !config_file_path.exists() {
            return Ok(Config::default());
        }

        // Read and parse the configuration file
        let config_str = fs::read_to_string(config_file_path)?;
        let config: Config = serde_json::from_str(&config_str)?;
        Ok(config)
    }

    /// Saves the current configuration to the filesystem.
    ///
    /// This method serializes the configuration to JSON format and writes it
    /// to the platform-specific application data directory. The JSON is
    /// formatted with proper indentation for human readability and manual editing.
    ///
    /// ## File Operations
    ///
    /// - Creates the application data directory if it doesn't exist
    /// - Overwrites any existing configuration file
    /// - Uses pretty-printing for readable JSON output
    /// - Sets appropriate file permissions for user-only access
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful save, or an error if the file cannot be written.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The application data directory cannot be created
    /// - The configuration file cannot be written due to permission issues
    /// - JSON serialization fails (should not happen with valid configurations)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::libs::config::{Config, MonitorConfig};
    ///
    /// let mut config = Config::read()?;
    /// config.monitor = Some(MonitorConfig::default());
    /// config.save()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn save(&self) -> Result<()> {
        // Resolve the configuration file path and ensure directory exists
        let config_file_path = DataStorage::new().get_path(CONFIG_FILE_NAME)?;
        // Serialize to JSON string first
        let json_content = serde_json::to_string_pretty(&self)?;
        // Write the JSON string to file and ensure it's flushed to disk
        fs::write(config_file_path, json_content)?;
        Ok(())
    }

    /// Returns the ignore list for task discovery.
    ///
    /// When `task_discovery` is not configured, returns the built-in defaults.
    pub fn effective_ignore_names(&self) -> Vec<String> {
        self.task_discovery
            .as_ref()
            .map(|c| c.ignore_names.clone())
            .unwrap_or_else(default_ignore_names)
    }

    /// Appends unique names to the discovery ignore list and saves the config.
    ///
    /// Uniqueness is determined by [`normalize_task_name`]. Returns how many
    /// new entries were added.
    pub fn add_ignore_names(&mut self, names: &[String]) -> Result<usize> {
        let mut discovery = self.task_discovery.clone().unwrap_or_default();
        let mut added = 0;

        for name in names {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = normalize_task_name(trimmed);
            let exists = discovery.ignore_names.iter().any(|existing| normalize_task_name(existing) == key);
            if !exists {
                discovery.ignore_names.push(trimmed.to_string());
                added += 1;
            }
        }

        self.task_discovery = Some(discovery);
        self.save()?;
        Ok(added)
    }

    /// Runs an interactive configuration setup wizard.
    ///
    /// This method provides a comprehensive guided setup experience that allows
    /// users to configure multiple application modules through an interactive
    /// command-line interface. The wizard presents available modules, collects
    /// configuration parameters, and validates inputs before saving.
    ///
    /// ## Setup Process
    ///
    /// 1. **Load Current Config**: Starts with existing configuration as defaults
    /// 2. **Module Selection**: Presents a multi-select list of available integrations
    /// 3. **Parameter Collection**: For each selected module, prompts for required settings
    /// 4. **Validation**: Performs basic validation on input parameters
    /// 5. **Configuration Return**: Returns the updated configuration for saving
    ///
    /// ## Available Modules
    ///
    /// - **SI (SearchInform)**: Company-specific API integration
    /// - **GitLab**: Source control integration for commit tracking
    /// - **Jira**: Issue tracking integration for task management
    /// - **Monitor**: Activity monitoring and pause detection settings
    /// - **Server**: External reporting API configuration
    ///
    /// ## User Experience
    ///
    /// - Uses colored prompts for better visual feedback
    /// - Pre-fills existing values as defaults to simplify updates
    /// - Provides helpful descriptions for each configuration parameter
    /// - Allows partial configuration (users can skip unwanted modules)
    ///
    /// # Returns
    ///
    /// Returns a fully configured `Config` instance ready for saving.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The existing configuration cannot be loaded
    /// - User input cannot be collected due to terminal issues
    /// - A module's configuration setup fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::libs::config::Config;
    ///
    /// // Run interactive setup and save the result
    /// let config = Config::init()?;
    /// config.save()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn init() -> Result<Self> {
        // The whole wizard is prompts: every module below asks for values, and
        // several read secrets. Without a terminal there is nobody to answer.
        crate::libs::prompt::ensure_interactive("`kasl init` is an interactive wizard and needs a terminal")?;

        // Load existing configuration to use as defaults for the setup wizard
        let mut config = Self::read().unwrap_or_default();

        // Define available configuration modules with their metadata
        let node_descriptions = [
            SiConfig::module(),
            GitLabConfig::module(),
            JiraConfig::module(),
            ConfigModule {
                key: "monitor".to_string(),
                name: "Monitor".to_string(),
            },
            ConfigModule {
                key: "server".to_string(),
                name: "Server".to_string(),
            },
            ConfigModule {
                key: "productivity".to_string(),
                name: "Productivity".to_string(),
            },
            ConfigModule {
                key: "report".to_string(),
                name: "Report".to_string(),
            },
            ConfigModule {
                key: "task_discovery".to_string(),
                name: "Task discovery".to_string(),
            },
            ConfigModule {
                key: "jira_inbox".to_string(),
                name: "Jira inbox".to_string(),
            },
        ];

        // Present multi-select interface for module selection
        let selected_nodes = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptSelectModules.to_string())
            .items(node_descriptions.iter().map(|module| &module.name).collect::<Vec<_>>())
            .interact()?;

        // Configure each selected module through its specific setup process
        for &selection in &selected_nodes {
            match node_descriptions[selection].key.as_str() {
                // External API integrations delegate to their own setup methods
                "si" => config.si = Some(SiConfig::init(&config.si)?),
                "gitlab" => config.gitlab = Some(GitLabConfig::init(&config.gitlab)?),
                "jira" => config.jira = Some(JiraConfig::init(&config.jira)?),

                // Monitor configuration uses inline setup for timing parameters
                "monitor" => {
                    let default = config.monitor.clone().unwrap_or_default();
                    msg_print!(Message::ConfigModuleMonitor);
                    config.monitor = Some(MonitorConfig {
                        // Minimum duration for recording pauses (reduces noise)
                        min_pause_duration: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptMinPauseDuration.to_string())
                            .default(default.min_pause_duration)
                            .interact_text()?,

                        // Inactivity threshold before pause detection begins
                        pause_threshold: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptPauseThreshold.to_string())
                            .default(default.pause_threshold)
                            .interact_text()?,

                        // Frequency of activity status checks
                        poll_interval: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptPollInterval.to_string())
                            .default(default.poll_interval)
                            .interact_text()?,

                        // Continuous activity required to start workday tracking
                        activity_threshold: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptActivityThreshold.to_string())
                            .default(default.activity_threshold)
                            .interact_text()?,

                        // Minimum interval duration for merging work blocks
                        min_work_interval: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptMinWorkInterval.to_string())
                            .default(default.min_work_interval)
                            .interact_text()?,

                        // Preserve the pause-merge gap (edited manually in config.json)
                        pause_merge_gap: default.pause_merge_gap,
                    });
                }

                // Server configuration for external report submission
                "server" => {
                    let default = config.server.clone().unwrap_or(ServerConfig {
                        api_url: "".to_string(),
                        auth_token: "".to_string(),
                    });
                    msg_print!(Message::ConfigModuleServer);
                    config.server = Some(ServerConfig {
                        // Base URL for the external reporting API
                        api_url: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptServerApiUrl.to_string())
                            .default(default.api_url)
                            .interact_text()?,

                        // Authentication token for API access
                        auth_token: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptServerAuthToken.to_string())
                            .default(default.auth_token)
                            .interact_text()?,
                    });
                }

                // Productivity configuration for break recommendations and reporting
                "productivity" => {
                    let default = config.productivity.clone().unwrap_or_default();
                    msg_print!(Message::ConfigModuleProductivity);
                    config.productivity = Some(ProductivityConfig {
                        // Minimum productivity percentage threshold
                        min_productivity_threshold: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptMinProductivityThreshold.to_string())
                            .default(default.min_productivity_threshold)
                            .interact_text()?,

                        // Expected workday duration
                        workday_hours: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptWorkdayHours.to_string())
                            .default(default.workday_hours)
                            .interact_text()?,

                        // Minimum workday fraction before the productivity warning
                        min_workday_fraction_before_suggest: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptMinWorkdayFraction.to_string())
                            .default(default.min_workday_fraction_before_suggest)
                            .interact_text()?,
                    });
                }

                // Report export configuration: default output directory and file name template
                "report" => {
                    let default = config.report.clone().unwrap_or(ReportConfig {
                        output_dir: None,
                        filename_template: None,
                        language: None,
                        template: None,
                    });
                    let output_dir: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Reports output directory")
                        .default(default.output_dir.unwrap_or_default())
                        .allow_empty(true)
                        .interact_text()?;
                    let filename_template: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Report file name template (placeholders: {date}, {seq})")
                        .default(default.filename_template.unwrap_or_else(|| "daily_report_{date}{seq}".to_string()))
                        .allow_empty(true)
                        .interact_text()?;
                    let language: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Report language (ru, en)")
                        .default(default.language.unwrap_or_else(|| "ru".to_string()))
                        .allow_empty(true)
                        .interact_text()?;
                    let template: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Report design template name")
                        .default(default.template.unwrap_or_else(|| "siserver".to_string()))
                        .allow_empty(true)
                        .interact_text()?;
                    config.report = Some(ReportConfig {
                        output_dir: if output_dir.trim().is_empty() { None } else { Some(output_dir) },
                        filename_template: if filename_template.trim().is_empty() { None } else { Some(filename_template) },
                        language: if language.trim().is_empty() { None } else { Some(language) },
                        template: if template.trim().is_empty() { None } else { Some(template) },
                    });
                }

                "task_discovery" => {
                    config.task_discovery = Some(configure_task_discovery(config.task_discovery.clone().unwrap_or_default())?);
                }

                "jira_inbox" => {
                    config.jira_inbox = Some(configure_jira_inbox(config.jira_inbox.clone().unwrap_or_default())?);
                }

                _ => {} // Unknown module keys are safely ignored
            }
        }

        Ok(config)
    }

    /// Adds the application to the global system PATH.
    ///
    /// This method ensures that the kasl executable can be run from any directory
    /// by adding its location to the system's PATH environment variable. This is
    /// particularly useful on Windows where applications are not automatically
    /// available globally after installation.
    ///
    /// ## Platform Behavior
    ///
    /// - **Windows**: Modifies the system registry to update the global PATH
    /// - **Unix-like**: Currently not implemented (uses shell integration instead)
    ///
    /// ## Windows Implementation Details
    ///
    /// The Windows implementation:
    /// 1. Determines the current executable's directory
    /// 2. Checks if the directory is already in the PATH
    /// 3. Updates the registry to add the directory if needed
    /// 4. Requires administrative privileges for system-wide changes
    ///
    /// ## Security Considerations
    ///
    /// - Modifying the system PATH requires elevated privileges on Windows
    /// - Changes affect all users on the system
    /// - The operation is reversible by manually editing the PATH
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the PATH was successfully updated or was already correct.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The current executable path cannot be determined
    /// - Registry operations fail due to insufficient privileges
    /// - System commands fail to execute properly
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::libs::config::Config;
    ///
    /// // Ensure kasl is available globally
    /// Config::set_app_global()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_app_global() -> Result<()> {
        // Get the directory containing the current executable
        let current_exe_path = env::current_exe()?;
        let exe_dir = current_exe_path.parent().unwrap();

        // Parse the current PATH environment variable
        let mut paths: Vec<PathBuf> = env::split_paths(&env::var_os("PATH").unwrap()).collect();
        let str_paths: Vec<&str> = paths.iter().filter_map(|p| p.to_str()).collect();

        // Check if the executable directory is already in PATH
        if str_paths.contains(&exe_dir.to_str().unwrap()) {
            return Ok(()); // Already configured, nothing to do
        }

        // Double-check using a different method to avoid duplicates
        if paths.iter().any(|p| p.to_str() == Some(exe_dir.to_str().unwrap())) {
            return Ok(()); // Already present, avoid duplication
        }

        // Add the executable directory to the PATH list
        paths.push(exe_dir.to_path_buf());

        // Reconstruct the PATH environment variable
        let new_path = env::join_paths(paths).unwrap_or_else(|_| panic!("{}", Message::FailedToJoinPaths.to_string()));

        // Define the Windows registry key for system environment variables
        let path_key = r"HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

        // Query the current PATH value from the Windows registry
        let reg_query_output = Command::new("reg")
            .arg("query")
            .arg(path_key)
            .arg("/v")
            .arg("Path")
            .output()
            .unwrap_or_else(|_| panic!("{}", Message::FailedToExecuteRegQuery.to_string()));

        // Handle registry query failures gracefully
        if !reg_query_output.status.success() {
            let status = reg_query_output.status.to_string();
            msg_error!(Message::PathRegistryQueryError { status: status.clone() });
            return Err(anyhow::anyhow!("{}", Message::PathRegistryQueryError { status }));
        }

        // Parse the current PATH value from registry output
        let current_path = str::from_utf8(&reg_query_output.stdout)
            .unwrap_or_else(|_| panic!("{}", Message::FailedToParseRegOutput.to_string()))
            .split_whitespace()
            .last()
            .unwrap_or_else(|| panic!("{}", Message::FailedToGetPathFromReg.to_string()));

        // Update the registry with the new PATH value
        let reg_set_output = Command::new("reg")
            .arg("add")
            .arg(path_key)
            .arg("/v")
            .arg("Path")
            .arg("/t")
            .arg("REG_EXPAND_SZ") // Expandable string type for environment variables
            .arg("/d")
            .arg(format!("{};{}", current_path, new_path.to_string_lossy()))
            .arg("/f") // Force overwrite without confirmation
            .output()
            .unwrap_or_else(|_| panic!("{}", Message::FailedToExecuteRegSet.to_string()));

        // Check if the registry update was successful
        if !reg_set_output.status.success() {
            let status = reg_set_output.status.to_string();
            let stderr = String::from_utf8_lossy(&reg_set_output.stderr).to_string();
            msg_error!(Message::PathRegistryUpdateError {
                status: status.clone(),
                stderr: stderr.clone()
            });
            return Err(anyhow::anyhow!("{}", Message::PathRegistryUpdateError { status, stderr }));
        }

        Ok(())
    }
}

/// Interactive editor for the task discovery ignore list.
fn configure_task_discovery(mut discovery: TaskDiscoveryConfig) -> Result<TaskDiscoveryConfig> {
    msg_print!(Message::ConfigModuleTaskDiscovery, true);

    if discovery.ignore_names.is_empty() {
        msg_info!(Message::TaskDiscoveryIgnoreListEmpty);
    } else {
        msg_print!(Message::TaskDiscoveryIgnoreListHeader, true);
        for (idx, name) in discovery.ignore_names.iter().enumerate() {
            println!("  {}. {}", idx + 1, name);
        }
        println!();

        let remove = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptSelectIgnoreNamesToRemove.to_string())
            .items(&discovery.ignore_names)
            .interact()?;

        if !remove.is_empty() {
            let mut keep = Vec::new();
            for (idx, name) in discovery.ignore_names.iter().enumerate() {
                if !remove.contains(&idx) {
                    keep.push(name.clone());
                }
            }
            discovery.ignore_names = keep;
        }
    }

    loop {
        let name: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptAddIgnoreName.to_string())
            .allow_empty(true)
            .interact_text()?;

        let trimmed = name.trim();
        if trimmed.is_empty() {
            break;
        }

        let key = normalize_task_name(trimmed);
        let exists = discovery.ignore_names.iter().any(|existing| normalize_task_name(existing) == key);
        if exists {
            msg_info!(Message::TaskDiscoveryIgnoreNameExists(trimmed.to_string()));
        } else {
            discovery.ignore_names.push(trimmed.to_string());
            msg_success!(Message::TaskDiscoveryIgnoreNameAdded(trimmed.to_string()));
        }
    }

    Ok(discovery)
}

/// Interactive wizard for Jira inbox polling settings.
fn configure_jira_inbox(default: JiraInboxConfig) -> Result<JiraInboxConfig> {
    msg_print!(Message::ConfigModuleJiraInbox, true);

    let enabled = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptJiraInboxEnabled.to_string())
        .default(default.enabled)
        .interact()?;

    let poll_interval_secs: u64 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptJiraInboxPollInterval.to_string())
        .default(default.poll_interval_secs)
        .interact_text()?;

    let notify = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptJiraInboxNotify.to_string())
        .default(default.notify)
        .interact()?;

    let default_sort_id = default.sort_by_field.clone().unwrap_or_default();
    let sort_field_id: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptJiraInboxSortFieldId.to_string())
        .with_initial_text(&default_sort_id)
        .allow_empty(true)
        .interact_text()?;

    let mut custom_fields = Vec::new();
    let mut sort_by_field = None;

    let sort_trimmed = sort_field_id.trim();
    if !sort_trimmed.is_empty() {
        let default_label = default
            .custom_fields
            .iter()
            .find(|f| f.id == sort_trimmed)
            .map(|f| f.label.clone())
            .unwrap_or_else(|| "Scoring".to_string());

        let sort_label: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptJiraInboxSortFieldLabel.to_string())
            .default(default_label)
            .interact_text()?;

        let label = {
            let t = sort_label.trim();
            if t.is_empty() { "Scoring".to_string() } else { t.to_string() }
        };
        custom_fields.push(JiraCustomField {
            id: sort_trimmed.to_string(),
            label,
        });
        sort_by_field = Some(sort_trimmed.to_string());
    }

    loop {
        let extra_id: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptJiraInboxExtraFieldId.to_string())
            .allow_empty(true)
            .interact_text()?;
        let trimmed = extra_id.trim();
        if trimmed.is_empty() {
            break;
        }
        if custom_fields.iter().any(|f| f.id == trimmed) {
            continue;
        }
        let extra_label: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptJiraInboxExtraFieldLabel.to_string())
            .default(trimmed.to_string())
            .interact_text()?;
        custom_fields.push(JiraCustomField {
            id: trimmed.to_string(),
            label: {
                let t = extra_label.trim();
                if t.is_empty() { trimmed.to_string() } else { t.to_string() }
            },
        });
    }

    Ok(JiraInboxConfig {
        enabled,
        poll_interval_secs: poll_interval_secs.max(30),
        notify,
        custom_fields,
        sort_by_field,
    })
}
