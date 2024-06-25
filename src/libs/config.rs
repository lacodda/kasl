use super::data_storage::DataStorage;
use crate::api::gitlab::GitLabConfig;
use crate::api::jira::JiraConfig;
use crate::api::si::SiConfig;
use dialoguer::{theme::ColorfulTheme, MultiSelect};
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub si: Option<SiConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gitlab: Option<GitLabConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jira: Option<JiraConfig>,
}

impl Config {
    pub fn read() -> Result<Config, Box<dyn Error>> {
        let config_file_path = DataStorage::new().get_path(CONFIG_FILE_NAME)?;
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
            Err(_) => Config {
                si: None,
                gitlab: None,
                jira: None,
            },
        };
        let node_descriptions = vec![SiConfig::module(), GitLabConfig::module(), JiraConfig::module()];
        let selected_nodes = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Select nodes to configure")
            .items(&node_descriptions.iter().map(|module| &module.name).collect::<Vec<_>>())
            .interact()?;

        for &selection in &selected_nodes {
            if SiConfig::module().key == node_descriptions[selection].key {
                config.si = Some(SiConfig::init(&config.si)?);
            }
            if GitLabConfig::module().key == node_descriptions[selection].key {
                config.gitlab = Some(GitLabConfig::init(&config.gitlab)?);
            }
            if JiraConfig::module().key == node_descriptions[selection].key {
                config.jira = Some(JiraConfig::init(&config.jira)?);
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
