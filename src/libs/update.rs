//! This module provides functionality for self-updating the application
//! by checking for new releases on GitHub, downloading, and replacing the binary.

use crate::libs::data_storage::DataStorage;
use crate::libs::messages::Message;
use crate::msg_info;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use flate2::read::GzDecoder;
use reqwest::Client;
use serde::Deserialize;
use std::env;
use std::fs::{self, File};
use std::path::PathBuf;
use tar::Archive;

// Include application metadata (name, version, owner) generated at build time.
include!(concat!(env!("OUT_DIR"), "/app_metadata.rs"));

const LAST_CHECK_FILE: &str = ".last_update_check";
const DAILY_CHECK_INTERVAL: i64 = 1; // Check for updates once a day.
const BACKUP_EXTENSION: &str = "bak";

/// Represents a GitHub release.
#[derive(Deserialize, Debug)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

/// Represents a single asset within a GitHub release.
#[derive(Deserialize, Debug)]
struct GitHubAsset {
    browser_download_url: String,
    name: String,
}

/// Manages the application update process.
#[derive(Debug)]
pub struct Updater {
    /// The HTTP client for making API requests.
    pub client: Client,
    /// The GitHub repository owner.
    pub owner: String,
    /// The application/repository name.
    pub name: String,
    /// The current version of the running application.
    pub version: String,
    /// The latest version available on GitHub, if newer.
    pub latest_version: Option<String>,
    /// The download URL for the latest release asset.
    pub download_url: Option<String>,
    /// The URL to fetch the latest GitHub release.
    releases_url: String,
    /// Path to the file storing the timestamp of the last update check.
    last_check_file: PathBuf,
}

impl Updater {
    /// Creates a new `Updater` instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the path to the data storage directory cannot be determined.
    pub fn new() -> Result<Self> {
        let owner = APP_METADATA_OWNER.to_owned();
        let name = APP_METADATA_NAME.to_owned();
        let last_check_file = DataStorage::new().get_path(LAST_CHECK_FILE)?;
        let releases_url = format!("https://api.github.com/repos/{}/{}/releases/latest", owner, name);

        Ok(Self {
            client: Client::new(),
            owner,
            name,
            version: APP_METADATA_VERSION.to_owned(),
            latest_version: None,
            download_url: None,
            last_check_file,
            releases_url,
        })
    }

    /// Displays a notification message if a new version is available.
    ///
    /// This function performs a throttled check and only shows a message
    /// if the check hasn't been performed recently.
    pub async fn show_update_notification() {
        let mut updater = match Self::new() {
            Ok(up) => up,
            Err(_) => return, // Fail silently if updater can't be initialized.
        };

        if !updater.is_check_due() {
            return;
        }

        if let Ok(true) = updater.check_for_latest_release().await {
            if let Some(latest_version) = &updater.latest_version {
                msg_info!(Message::UpdateAvailable{ app_name: updater.name, latest: latest_version.to_string() }, true)
            }
        }
    }

    /// Performs the full update process: download, extract, and replace.
    ///
    /// # Preconditions
    ///
    /// This method assumes `check_for_latest_release` has been called and
    /// `self.download_url` is `Some`.
    ///
    /// # Errors
    ///
    /// Returns an error if downloading, file I/O, or extraction fails.
    pub async fn perform_update(&self) -> Result<()> {
        let download_url = self.download_url.as_ref().ok_or(anyhow::anyhow!("Download URL not set"))?;

        // Download the release asset (tar.gz).
        let response = self.client.get(download_url).send().await?;
        let content = response.bytes().await?;

        // Save the archive to a temporary file.
        let tar_gz_path = env::temp_dir().join(format!("{}.tar.gz", &self.name));
        fs::write(&tar_gz_path, &content)?;

        // Extract the binary and replace the current one.
        self.extract_and_replace_binary(&tar_gz_path)?;

        // Clean up the downloaded archive.
        fs::remove_file(&tar_gz_path)?;

        Ok(())
    }

    /// Fetches the latest release information from GitHub and updates the updater's state.
    ///
    /// Returns `true` if a newer version is available, `false` otherwise.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request or JSON deserialization fails.
    pub async fn check_for_latest_release(&mut self) -> Result<bool> {
        let release = self.fetch_latest_github_release().await?;
        self.update_last_check_time();

        let latest_version = release.tag_name.trim_start_matches('v').to_string();

        if latest_version > self.version {
            self.latest_version = Some(latest_version);
            self.download_url = self.find_platform_asset_url(&release.assets).map(|url| url.to_string());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Fetches release data from the GitHub API.
    async fn fetch_latest_github_release(&self) -> Result<GitHubRelease, reqwest::Error> {
        self.client
            .get(&self.releases_url)
            .header("User-Agent", &self.name)
            .send()
            .await?
            .json::<GitHubRelease>()
            .await
    }

    /// Finds the correct asset URL for the current platform architecture and OS.
    fn find_platform_asset_url<'a>(&self, assets: &'a [GitHubAsset]) -> Option<&'a str> {
        let platform_name = self.get_platform_identifier();
        assets
            .iter()
            .find(|asset| asset.name.contains(&platform_name))
            .map(|asset| asset.browser_download_url.as_str())
    }

    // Extracts the new binary from the downloaded archive and replaces the current executable.
    fn extract_and_replace_binary(&self, tar_gz_path: &PathBuf) -> Result<()> {
        let tar_gz = File::open(tar_gz_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        let mut is_updated = false;

        let current_exe = env::current_exe()?;
        let current_exe_backup = current_exe.with_extension(BACKUP_EXTENSION);

        for entry_result in archive.entries()? {
            let mut entry = entry_result?;
            let entry_path = entry.path()?;
            if entry_path.ends_with(current_exe.file_name().unwrap()) {
                // Backup the current executable before replacing it.
                fs::rename(&current_exe, &current_exe_backup)?;
                // Extract new executable to the current executable location
                entry.unpack(&current_exe)?;
                is_updated = true;
            } else {
                // Extract other files to the same directory as the executable
                let dest_path = current_exe.parent().unwrap().join(&entry_path);
                entry.unpack(dest_path)?;
            }
        }

        if is_updated {
            Ok(())
        } else {
            anyhow::bail!("Binary not found in the release archive.")
        }
    }

    /// Constructs the platform-specific identifier used in release asset names.
    fn get_platform_identifier(&self) -> String {
        let arch = env::consts::ARCH;
        let os = match env::consts::OS {
            "windows" => "pc-windows-msvc",
            "macos" => "apple-darwin",
            _ => "unknown-linux-musl",
        };
        // Example: "x86_64-pc-windows-msvc"
        format!("{}-{}", arch, os)
    }

    /// Updates the timestamp in the `.last_update_check` file to the current time.
    fn update_last_check_time(&self) {
        let now = Utc::now().to_rfc3339();
        // Ignoring the result is acceptable here as failing to write the check time
        // is not a critical error. The check will simply run again next time.
        let _ = fs::write(&self.last_check_file, now);
    }

    /// Determines if enough time has passed to warrant a new check.
    fn is_check_due(&self) -> bool {
        match fs::read_to_string(&self.last_check_file) {
            Ok(content) => {
                let last_check = content
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now() - Duration::days(DAILY_CHECK_INTERVAL + 1));
                Utc::now().signed_duration_since(last_check) > Duration::days(DAILY_CHECK_INTERVAL)
            }
            Err(_) => true, // If file doesn't exist or can't be read, always check.
        }
    }
}
