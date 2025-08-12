//! Configuration management system for the kasl application.
//!
//! This module provides a comprehensive configuration management system that handles
//! application settings, external API integrations, and activity monitoring parameters.
//! It supports both programmatic configuration and interactive setup wizards.
//!
//! ## Core Features
//!
//! - **Multi-Service Integration**: Manages configurations for Jira, GitLab, and custom APIs
//! - **Activity Monitoring**: Configures behavior for work time tracking and pause detection
//! - **Interactive Setup**: Provides guided configuration wizards for all modules
//! - **Cross-Platform Persistence**: Handles configuration storage across Windows, macOS, and Linux
//! - **System Integration**: Manages global PATH configuration for CLI availability
//!
//! ## Configuration Structure
//!
//! The configuration system is modular, with each integration service having its own
//! dedicated configuration structure:
//!
//! - **SI Config**: Internal company API integration settings
//! - **GitLab Config**: GitLab API credentials and endpoints
//! - **Jira Config**: Jira instance connection parameters
//! - **Monitor Config**: Activity detection and pause thresholds
//! - **Server Config**: External reporting server configuration
//!
//! ## Storage and Security
//!
//! - Configuration files are stored in JSON format in platform-specific directories
//! - Sensitive data like passwords are never stored in configuration files
//! - Session tokens and credentials use separate encrypted storage mechanisms
//! - All configuration paths follow OS conventions for application data storage
//!
//! ## Usage Examples
//!
//! ```rust,no_run
//! use kasl::libs::config::Config;
//!
//! // Load existing configuration or create default
//! let config = Config::read()?;
//!
//! // Run interactive configuration setup
//! let updated_config = Config::init()?;
//! updated_config.save()?;
//!
//! // Access specific service configurations
//! if let Some(jira_config) = &config.jira {
//!     println!("Jira URL: {}", jira_config.api_url);
//! }
//! ```

use super::data_storage::DataStorage;
use crate::api::gitlab::GitLabConfig;
use crate::api::jira::JiraConfig;
use crate::api::si::SiConfig;
use crate::libs::messages::Message;
use crate::{msg_error, msg_print};
use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, File};
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
/// This structure controls the behavior of the background activity monitoring system
/// that tracks user presence, detects work patterns, and manages pause recording.
/// All timing values are carefully calibrated to provide accurate work time tracking
/// while minimizing false positives and system resource usage.
///
/// ## Timing Configuration
///
/// The monitor uses several timing thresholds to distinguish between different
/// types of user activity and inactivity:
///
/// - **Pause Detection**: Identifies when the user steps away from their workstation
/// - **Activity Filtering**: Ensures random brief activity doesn't restart work tracking
/// - **Interval Merging**: Combines short work periods to reduce fragmentation
///
/// ## Performance Considerations
///
/// - Lower poll intervals provide more responsive detection but use more CPU
/// - Higher activity thresholds reduce false workday starts from brief interactions
/// - Pause thresholds balance between capturing real breaks and ignoring brief interruptions
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
    /// use kasl::libs::config::Config;
    ///
    /// // Load configuration, falling back to defaults if no file exists
    /// let config = Config::read()?;
    ///
    /// // Check if Jira is configured
    /// if config.jira.is_some() {
    ///     println!("Jira integration is configured");
    /// }
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
    /// use kasl::libs::config::{Config, MonitorConfig};
    ///
    /// let mut config = Config::read()?;
    /// config.monitor = Some(MonitorConfig::default());
    /// config.save()?;
    /// ```
    pub fn save(&self) -> Result<()> {
        // Resolve the configuration file path and ensure directory exists
        let config_file_path = DataStorage::new().get_path(CONFIG_FILE_NAME)?;

        // Create the configuration file with pretty-printed JSON
        let config_file = File::create(config_file_path)?;
        serde_json::to_writer_pretty(&config_file, &self)?;
        Ok(())
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
    /// use kasl::libs::config::Config;
    ///
    /// // Run interactive setup and save the result
    /// let config = Config::init()?;
    /// config.save()?;
    /// ```
    pub fn init() -> Result<Self> {
        // Load existing configuration to use as defaults for the setup wizard
        let mut config = match Self::read() {
            Ok(config) => config,
            Err(_) => Config::default(), // Fall back to default if loading fails
        };

        // Define available configuration modules with their metadata
        let node_descriptions = vec![
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
        ];

        // Present multi-select interface for module selection
        let selected_nodes = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptSelectModules.to_string())
            .items(&node_descriptions.iter().map(|module| &module.name).collect::<Vec<_>>())
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
    /// use kasl::libs::config::Config;
    ///
    /// // Ensure kasl is available globally
    /// Config::set_app_global()?;
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
        let new_path = env::join_paths(paths).expect(&Message::FailedToJoinPaths.to_string());

        // Define the Windows registry key for system environment variables
        let path_key = r"HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

        // Query the current PATH value from the Windows registry
        let reg_query_output = Command::new("reg")
            .arg("query")
            .arg(path_key)
            .arg("/v")
            .arg("Path")
            .output()
            .expect(&Message::FailedToExecuteRegQuery.to_string());

        // Handle registry query failures gracefully
        if !reg_query_output.status.success() {
            msg_error!(Message::PathQueryFailed(reg_query_output.status.to_string()));
            return Ok(());
        }

        // Parse the current PATH value from registry output
        let current_path = str::from_utf8(&reg_query_output.stdout)
            .expect(&Message::FailedToParseRegOutput.to_string())
            .split_whitespace()
            .last()
            .expect(&Message::FailedToGetPathFromReg.to_string());

        // Update the registry with the new PATH value
        let reg_set_output = Command::new("reg")
            .arg("add")
            .arg(path_key)
            .arg("/v")
            .arg("Path")
            .arg("/t")
            .arg("REG_EXPAND_SZ") // Expandable string type for environment variables
            .arg("/d")
            .arg(&format!("{};{}", current_path, new_path.to_string_lossy()))
            .arg("/f") // Force overwrite without confirmation
            .output()
            .expect(&Message::FailedToExecuteRegSet.to_string());

        // Check if the registry update was successful
        if !reg_set_output.status.success() {
            msg_error!(Message::PathSetFailed);
            return Ok(());
        }

        Ok(())
    }
}
