//! Application configuration: the data model, disk IO, and PATH setup.
//! The interactive setup wizard lives in [`wizard`] (reached via
//! [`Config::init`]).
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

mod wizard;

use super::data_storage::DataStorage;
use crate::api::gitlab::GitLabConfig;
use crate::api::jira::JiraConfig;
use crate::api::si::SiConfig;
use crate::libs::messages::Message;
use crate::libs::task::normalize_task_name;
use crate::msg_error;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::str;

/// Configuration filename inside the app data directory.
pub const CONFIG_FILE_NAME: &str = "config.json";

/// A configurable module as listed by the setup wizard.
#[derive(Debug, Clone)]
pub struct ConfigModule {
    /// Internal key used for routing.
    pub key: String,
    /// Name shown in the wizard.
    pub name: String,
}

/// Activity monitor thresholds and intervals.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MonitorConfig {
    /// Minimum pause length in minutes to keep as a pause record; shorter
    /// interruptions are noise, not breaks.
    pub min_pause_duration: u64,

    /// Seconds without input before a pause is detected.
    pub pause_threshold: u64,

    /// Milliseconds between activity checks.
    pub poll_interval: u64,

    /// Seconds of sustained activity required to start a workday - keeps
    /// a stray mouse nudge from opening the day.
    pub activity_threshold: u64,

    /// Minimum work interval in minutes; shorter ones are filtered from
    /// report display.
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

/// Productivity thresholds for warnings and report validation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProductivityConfig {
    /// Productivity percentage below which reports warn.
    pub min_productivity_threshold: f64,

    /// Expected workday length in hours.
    pub workday_hours: f64,

    /// Fraction of the workday that must pass before warnings appear -
    /// early-day ratios swing too wildly to act on.
    pub min_workday_fraction_before_suggest: f64,
}

/// Report export defaults: output directory and file naming.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ReportConfig {
    /// Default directory for exports without an explicit `--output`
    /// (created if missing); unset = timestamped file in the current dir.
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

    /// Report label language: `ru` (default) or `en`; unknown values fall
    /// back to `ru`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Design template name (`<data>/report_templates/<name>.json`);
    /// unset or missing falls back to the built-in `siserver` look.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
}

/// External reporting server connection.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServerConfig {
    /// Base URL of the reporting API.
    pub api_url: String,

    /// Token sent with report submissions.
    pub auth_token: String,
}

/// The root configuration. Every module is optional, and unset modules
/// are omitted from the JSON, so the file only names what is configured.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Config {
    /// SiServer integration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub si: Option<SiConfig>,

    /// GitLab integration (commit discovery).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gitlab: Option<GitLabConfig>,

    /// Jira integration (issue discovery, inbox).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jira: Option<JiraConfig>,

    /// Activity monitor settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<MonitorConfig>,

    /// External reporting server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerConfig>,

    /// Productivity thresholds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub productivity: Option<ProductivityConfig>,

    /// Report export defaults.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<ReportConfig>,

    /// Task discovery ignore list; built-in defaults apply when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_discovery: Option<TaskDiscoveryConfig>,

    /// Jira inbox polling; requires `jira`, disabled when absent.
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

    /// Whether to show a toast when an existing issue visibly changes
    /// (status, priority, score).
    #[serde(default = "default_true")]
    pub notify_changes: bool,

    /// Whether to show a toast when an issue leaves the inbox
    /// (closed or reassigned). Off by default.
    #[serde(default)]
    pub notify_gone: bool,

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
            notify_changes: true,
            notify_gone: false,
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
    fn default() -> Self {
        ProductivityConfig {
            min_productivity_threshold: 75.0,
            workday_hours: 8.0,
            min_workday_fraction_before_suggest: 0.5,
        }
    }
}

impl Config {
    /// Loads the config file, or defaults when none exists yet.
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::libs::config::Config;
    ///
    /// let config = Config::read()?;
    ///
    /// if config.jira.is_some() {
    ///     println!("Jira integration is configured");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn read() -> Result<Config> {
        let config_file_path = DataStorage::new().get_path(CONFIG_FILE_NAME)?;

        if !config_file_path.exists() {
            return Ok(Config::default());
        }

        let config_str = fs::read_to_string(config_file_path)?;
        let config: Config = serde_json::from_str(&config_str)?;
        Ok(config)
    }

    /// Writes the config as pretty-printed JSON, overwriting the file.
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
        let config_file_path = DataStorage::new().get_path(CONFIG_FILE_NAME)?;
        let json_content = serde_json::to_string_pretty(&self)?;
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

    /// Adds the executable's directory to the global PATH.
    ///
    /// Windows-shaped: checks the process PATH first, then edits the
    /// machine-level registry value via `reg` (which needs admin rights;
    /// the setup command downgrades a failure here to a warning).
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::libs::config::Config;
    ///
    /// Config::set_app_global()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_app_global() -> Result<()> {
        let current_exe_path = env::current_exe()?;
        let exe_dir = current_exe_path.parent().unwrap();

        let mut paths: Vec<PathBuf> = env::split_paths(&env::var_os("PATH").unwrap()).collect();
        let str_paths: Vec<&str> = paths.iter().filter_map(|p| p.to_str()).collect();

        if str_paths.contains(&exe_dir.to_str().unwrap()) {
            return Ok(());
        }

        if paths.iter().any(|p| p.to_str() == Some(exe_dir.to_str().unwrap())) {
            return Ok(());
        }

        paths.push(exe_dir.to_path_buf());

        let new_path = env::join_paths(paths).unwrap_or_else(|_| panic!("{}", Message::FailedToJoinPaths.to_string()));

        let path_key = r"HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

        let reg_query_output = Command::new("reg")
            .arg("query")
            .arg(path_key)
            .arg("/v")
            .arg("Path")
            .output()
            .unwrap_or_else(|_| panic!("{}", Message::FailedToExecuteRegQuery.to_string()));

        if !reg_query_output.status.success() {
            let status = reg_query_output.status.to_string();
            msg_error!(Message::PathRegistryQueryError { status: status.clone() });
            return Err(anyhow::anyhow!("{}", Message::PathRegistryQueryError { status }));
        }

        let current_path = str::from_utf8(&reg_query_output.stdout)
            .unwrap_or_else(|_| panic!("{}", Message::FailedToParseRegOutput.to_string()))
            .split_whitespace()
            .last()
            .unwrap_or_else(|| panic!("{}", Message::FailedToGetPathFromReg.to_string()));

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
