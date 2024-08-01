use serde::Deserialize;
use std::env::consts::OS;
use std::env::var;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::{fs, str};
include!(concat!(env!("OUT_DIR"), "/app_metadata.rs"));

#[derive(Deserialize, Clone)]
pub struct DataStorage {
    base_path: PathBuf,
}

impl DataStorage {
    pub fn new() -> Self {
        let base_path = match OS {
            "windows" => var("LOCALAPPDATA").unwrap_or_else(|_| ".".into()),
            "macos" => var("HOME").unwrap_or_else(|_| ".".into()) + "/Library/Application Support",
            _ => var("HOME").unwrap_or_else(|_| ".".into()) + "/.local/share",
        };
        let base_path = Path::new(&base_path).join(APP_METADATA_OWNER).join(APP_METADATA_NAME);

        Self { base_path }
    }

    pub fn get_path(&self, file_name: &str) -> Result<PathBuf, Box<dyn Error>> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path)?;
        }
        Ok(self.base_path.join(file_name))
    }
}
