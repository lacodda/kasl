use super::data_storage::DataStorage;
use serde::Deserialize;
use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::str;

pub const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Deserialize, Clone)]
pub struct SiConfig {
    pub login: String,
    pub auth_url: String,
    pub api_url: String,
}

#[derive(Deserialize, Clone)]
pub struct Config {
    pub si: SiConfig,
}

impl Config {
    pub fn read() -> Result<Config, Box<dyn Error>> {
        let config_file_path = DataStorage::new().get_path(CONFIG_FILE_NAME)?;
        let config_str = fs::read_to_string(config_file_path)?;
        let config: Config = serde_json::from_str(&config_str)?;

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
