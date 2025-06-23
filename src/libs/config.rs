use super::data_storage::DataStorage;
use crate::api::gitlab::GitLabConfig;
use crate::api::jira::JiraConfig;
use crate::api::si::SiConfig;
use dialoguer::{theme::ColorfulTheme, Input, MultiSelect};
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::path::PathBuf;
use std::process::Command;
use std::str;

pub const CONFIG_FILE_NAME: &str = "config.json";

pub struct ConfigModule {
    pub key: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MonitorConfig {
    pub min_break_duration: u64, // Minimum break duration in minutes
    pub break_threshold: u64,    // Inactivity threshold in seconds
    pub poll_interval: u64,      // Poll interval in milliseconds
    pub activity_threshold: u64, // Activity duration threshold in seconds
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServerConfig {
    pub api_url: String,
    pub auth_token: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub si: Option<SiConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gitlab: Option<GitLabConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jira: Option<JiraConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<MonitorConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerConfig>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        MonitorConfig {
            min_break_duration: 20, // 20 minutes
            break_threshold: 60,    // 60 seconds
            poll_interval: 500,     // 500 milliseconds
            activity_threshold: 30, // 30 seconds
        }
    }
}

impl Config {
    pub fn read() -> Result<Config, Box<dyn Error>> {
        let config_file_path = DataStorage::new().get_path(CONFIG_FILE_NAME)?;
        if !config_file_path.exists() {
            return Ok(Config::default());
        }
        let config_str = fs::read_to_string(config_file_path)?;
        let config: Config = serde_json::from_str(&config_str)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        let config_file_path = DataStorage::new().get_path(CONFIG_FILE_NAME)?;
        let config_file = File::create(config_file_path)?;
        serde_json::to_writer_pretty(&config_file, &self)?;
        Ok(())
    }

    pub fn init() -> Result<Self, Box<dyn Error>> {
        let mut config = match Self::read() {
            Ok(config) => config,
            Err(_) => Config::default(),
        };
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
        let selected_nodes = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Select nodes to configure")
            .items(&node_descriptions.iter().map(|module| &module.name).collect::<Vec<_>>())
            .interact()?;

        for &selection in &selected_nodes {
            match node_descriptions[selection].key.as_str() {
                "si" => config.si = Some(SiConfig::init(&config.si)?),
                "gitlab" => config.gitlab = Some(GitLabConfig::init(&config.gitlab)?),
                "jira" => config.jira = Some(JiraConfig::init(&config.jira)?),
                "monitor" => {
                    let default = config.monitor.clone().unwrap_or_default();
                    println!("Monitor settings");
                    config.monitor = Some(MonitorConfig {
                        min_break_duration: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt("Enter minimum break duration (minutes)")
                            .default(default.min_break_duration)
                            .interact_text()?,
                        break_threshold: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt("Enter break threshold (seconds)")
                            .default(default.break_threshold)
                            .interact_text()?,
                        poll_interval: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt("Enter poll interval (milliseconds)")
                            .default(default.poll_interval)
                            .interact_text()?,
                        activity_threshold: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt("Enter activity threshold (seconds)")
                            .default(default.activity_threshold)
                            .interact_text()?,
                    });
                }
                "server" => {
                    let default = config.server.clone().unwrap_or(ServerConfig {
                        api_url: "".to_string(),
                        auth_token: "".to_string(),
                    });
                    println!("Server settings");
                    config.server = Some(ServerConfig {
                        api_url: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt("Enter server API URL")
                            .default(default.api_url)
                            .interact_text()?,
                        auth_token: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt("Enter server auth token")
                            .default(default.auth_token)
                            .interact_text()?,
                    });
                }
                _ => {}
            }
        }

        Ok(config)
    }

    pub fn set_app_global() -> Result<(), Box<dyn Error>> {
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

        let new_path = env::join_paths(paths).expect("Failed to join paths");
        let path_key = r"HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

        let reg_query_output = Command::new("reg")
            .arg("query")
            .arg(path_key)
            .arg("/v")
            .arg("Path")
            .output()
            .expect("Failed to execute reg query");

        if !reg_query_output.status.success() {
            println!("Failed to query PATH from registry: {:?}", reg_query_output.status);
            return Ok(());
        }

        let current_path = str::from_utf8(&reg_query_output.stdout)
            .expect("Failed to parse reg query output")
            .split_whitespace()
            .last()
            .expect("Failed to get PATH value from reg query");

        let reg_set_output = Command::new("reg")
            .arg("add")
            .arg(path_key)
            .arg("/v")
            .arg("Path")
            .arg("/t")
            .arg("REG_EXPAND_SZ")
            .arg("/d")
            .arg(&format!("{};{}", current_path, new_path.to_string_lossy()))
            .arg("/f")
            .output()
            .expect("Failed to execute reg set");

        if !reg_set_output.status.success() {
            println!("Failed to set PATH in registry");
            return Ok(());
        }

        Ok(())
    }
}

impl Default for Config {
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
