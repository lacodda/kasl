//! Resolves where application files live on each platform.
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use kasl::libs::data_storage::DataStorage;
//!
//! let storage = DataStorage::new();
//! let db_path = storage.get_path("kasl.db")?;
//! let config_path = storage.get_path("config.json")?;
//! # Ok(())
//! # }
//! ```

use anyhow::Result;
use serde::Deserialize;
use std::env::consts::OS;
use std::env::var;
use std::path::{Path, PathBuf};
use std::{fs, str};

// Include compile-time application metadata
include!(concat!(env!("OUT_DIR"), "/app_metadata.rs"));

/// The application data directory, resolved once at construction.
#[derive(Deserialize, Clone)]
pub struct DataStorage {
    /// `{platform data dir}/{owner}/{app}`; files are resolved under it.
    base_path: PathBuf,
}

impl Default for DataStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl DataStorage {
    /// Picks the platform data directory and appends owner and app name
    /// (from compile-time metadata).
    ///
    /// `LOCALAPPDATA` on Windows, `~/Library/Application Support` on macOS,
    /// `~/.local/share` elsewhere; falls back to `.` when the environment
    /// variable is missing, so restricted environments still run.
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::libs::data_storage::DataStorage;
    ///
    /// let storage = DataStorage::new();
    /// let db_path = storage.get_path("kasl.db")?;
    /// println!("Database path: {:?}", db_path);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Self {
        let base_path = match OS {
            "windows" => var("LOCALAPPDATA").unwrap_or_else(|_| ".".into()),
            "macos" => var("HOME").unwrap_or_else(|_| ".".into()) + "/Library/Application Support",
            _ => var("HOME").unwrap_or_else(|_| ".".into()) + "/.local/share",
        };

        let base_path = Path::new(&base_path).join(APP_METADATA_OWNER).join(APP_METADATA_NAME);

        Self { base_path }
    }

    /// Returns the full path for `file_name` inside the data directory,
    /// creating the directory tree on first use.
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::libs::data_storage::DataStorage;
    ///
    /// let storage = DataStorage::new();
    ///
    /// let db_path = storage.get_path("kasl.db")?;
    /// // /home/user/.local/share/lacodda/kasl/kasl.db (Linux)
    /// // C:\Users\User\AppData\Local\lacodda\kasl\kasl.db (Windows)
    ///
    /// let config_path = storage.get_path("config.json")?;
    /// let session_path = storage.get_path(".jira_session_id")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_path(&self, file_name: &str) -> Result<PathBuf> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path)?;
        }
        Ok(self.base_path.join(file_name))
    }
}
