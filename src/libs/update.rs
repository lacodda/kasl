//! Self-update from GitHub releases.
//!
//! ```rust,no_run
//! use kasl::libs::update::Updater;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut updater = Updater::new()?;
//!
//!     if updater.check_for_latest_release().await? {
//!         updater.perform_update().await?;
//!     }
//!
//!     Ok(())
//! }
//! ```

use crate::libs::data_storage::DataStorage;
use crate::libs::messages::Message;
use crate::{msg_bail_anyhow, msg_error_anyhow, msg_info};
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use flate2::read::GzDecoder;
use reqwest::Client;
use std::env;
use std::fs::{self, File};
use std::path::PathBuf;
use tar::Archive;

// Include application metadata (name, version, owner) generated at build time.
include!(concat!(env!("OUT_DIR"), "/app_metadata.rs"));

/// Cache file holding the timestamp of the last update check.
const LAST_CHECK_FILE: &str = ".last_update_check";

/// Minimum days between startup update checks.
const DAILY_CHECK_INTERVAL: i64 = 1;

/// Extension the replaced executable is kept under (`kasl.bak`).
const BACKUP_EXTENSION: &str = "bak";

/// The update workflow: check the latest tag, download the platform asset,
/// swap the binary keeping the old one as `.bak`.
#[derive(Debug)]
pub struct Updater {
    pub client: Client,

    /// Repository owner, from build-time metadata.
    pub owner: String,

    /// Repository/app name, from build-time metadata.
    pub name: String,

    /// Version of the running binary.
    pub version: String,

    /// Newer version found by the check, if any.
    pub latest_version: Option<String>,

    /// Asset URL for this platform, set when a newer version is found.
    pub download_url: Option<String>,

    /// URL of the repository's `releases/latest` page.
    ///
    /// The latest tag is read from this page's redirect `Location` header
    /// instead of `api.github.com`: the API allows only 60 anonymous
    /// requests per hour per IP, which starves every machine behind a
    /// shared NAT (the same failure the installers hit).
    releases_url: String,

    /// Path of the check-throttling timestamp file.
    last_check_file: PathBuf,
}

impl Updater {
    /// Builds an updater from build-time metadata.
    ///
    /// ```rust,no_run
    /// # fn f() -> anyhow::Result<()> {
    /// use kasl::libs::update::Updater;
    ///
    /// let updater = Updater::new()?;
    /// println!("Updater configured for {} v{}", updater.name, updater.version);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let owner = APP_METADATA_OWNER.to_owned();
        let name = APP_METADATA_NAME.to_owned();

        let last_check_file = DataStorage::new().get_path(LAST_CHECK_FILE)?;

        // Release page whose redirect reveals the latest tag (no API quota)
        let releases_url = format!("https://github.com/{}/{}/releases/latest", owner, name);

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

    /// Prints an update notice when one is available - throttled to one
    /// check per day, and silent on any failure, so startup never blocks
    /// or complains because of the network.
    ///
    /// ```rust,no_run
    /// # async fn f() {
    /// use kasl::libs::update::Updater;
    ///
    /// // Call during application startup
    /// Updater::show_update_notification().await;
    /// # }
    /// ```
    pub async fn show_update_notification() {
        let mut updater = match Self::new() {
            Ok(up) => up,
            Err(_) => return,
        };

        if !updater.is_check_due() {
            return;
        }

        if let Ok(true) = updater.check_for_latest_release().await
            && let Some(latest_version) = &updater.latest_version
        {
            msg_info!(
                Message::UpdateAvailable {
                    app_name: updater.name,
                    latest: latest_version.to_string()
                },
                true // Show with extra spacing for visibility
            )
        }
    }

    /// Downloads the release archive and swaps the binary in.
    ///
    /// Requires a prior successful [`Updater::check_for_latest_release`]
    /// (it sets `download_url`). The old executable stays next to the new
    /// one as `.bak` - restoring it is a manual copy, nothing automatic.
    ///
    /// ```rust,no_run
    /// # async fn f() -> anyhow::Result<()> {
    /// use kasl::libs::update::Updater;
    ///
    /// let mut updater = Updater::new()?;
    /// if updater.check_for_latest_release().await? {
    ///     updater.perform_update().await?;
    ///     println!("Update completed successfully");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn perform_update(&self) -> Result<()> {
        let download_url = self.download_url.as_ref().ok_or(msg_error_anyhow!(Message::UpdateDownloadUrlNotSet))?;

        let response = self.client.get(download_url).send().await?;
        let content = response.bytes().await?;

        let tar_gz_path = env::temp_dir().join(format!("{}.tar.gz", self.name));
        fs::write(&tar_gz_path, &content)?;

        self.extract_and_replace_binary(&tar_gz_path)?;

        fs::remove_file(&tar_gz_path)?;

        Ok(())
    }

    /// Compares the latest published tag against the running version;
    /// on a newer one, stores it and the platform asset URL.
    ///
    /// ```rust,no_run
    /// # async fn f() -> anyhow::Result<()> {
    /// use kasl::libs::update::Updater;
    ///
    /// let mut updater = Updater::new()?;
    /// if updater.check_for_latest_release().await? {
    ///     println!("Update available: {} -> {}",
    ///         updater.version,
    ///         updater.latest_version.unwrap());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn check_for_latest_release(&mut self) -> Result<bool> {
        let tag = self.fetch_latest_tag().await?;

        self.update_last_check_time();

        let latest_version = tag.trim_start_matches('v').to_string();

        // String comparison; adequate for this project's version scheme.
        if latest_version > self.version {
            // Asset names follow the release convention: {name}-{tag}-{platform}.tar.gz
            self.download_url = Some(format!(
                "https://github.com/{}/{}/releases/download/{}/{}-{}-{}.tar.gz",
                self.owner,
                self.name,
                tag,
                self.name,
                tag,
                self.get_platform_identifier()
            ));
            self.latest_version = Some(latest_version);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Reads the latest release tag from the `releases/latest` redirect.
    ///
    /// GitHub answers this page with a `302` to `.../releases/tag/<tag>`;
    /// the tag is taken from the `Location` header. Unlike `api.github.com`,
    /// this endpoint has no anonymous rate limit, so it keeps working for
    /// every machine behind a shared NAT.
    async fn fetch_latest_tag(&self) -> Result<String> {
        // The shared client follows redirects (needed for asset downloads),
        // so the redirect probe uses its own non-following client.
        let client = Client::builder().redirect(reqwest::redirect::Policy::none()).build()?;
        let response = client.get(&self.releases_url).header("User-Agent", &self.name).send().await?;

        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| msg_error_anyhow!(Message::UpdateLatestTagNotFound(self.releases_url.clone())))?;

        match location.rsplit_once("/releases/tag/") {
            Some((_, tag)) if !tag.is_empty() => Ok(tag.to_string()),
            _ => Err(msg_error_anyhow!(Message::UpdateLatestTagNotFound(self.releases_url.clone()))),
        }
    }

    /// Unpacks the archive: the entry matching the current executable's
    /// name replaces it (old binary renamed to `.bak` first), everything
    /// else lands next to it. Errors if the archive holds no executable.
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
                // Keep the running binary as the one-and-only backup.
                fs::rename(&current_exe, &current_exe_backup)?;
                entry.unpack(&current_exe)?;
                is_updated = true;
            } else {
                let dest_path = current_exe.parent().unwrap().join(&entry_path);
                entry.unpack(dest_path)?;
            }
        }

        if is_updated {
            Ok(())
        } else {
            msg_bail_anyhow!(Message::UpdateBinaryNotFoundInArchive);
        }
    }

    /// Target triple used in release asset names, e.g.
    /// `x86_64-pc-windows-msvc`, `aarch64-apple-darwin`,
    /// `x86_64-unknown-linux-gnu`.
    fn get_platform_identifier(&self) -> String {
        let arch = env::consts::ARCH;
        let os = match env::consts::OS {
            "windows" => "pc-windows-msvc",
            "macos" => "apple-darwin",
            // Must match the published asset triple; releases ship glibc
            // builds (the installers hit 404s on the old musl guess).
            _ => "unknown-linux-gnu",
        };

        format!("{}-{}", arch, os)
    }

    /// Stamps the throttle file; write errors are ignored on purpose -
    /// throttling is a convenience, and a failed write only means one
    /// extra check later.
    fn update_last_check_time(&self) {
        let now = Utc::now().to_rfc3339();
        let _ = fs::write(&self.last_check_file, now);
    }

    /// True when the daily check interval has passed. Fails open: a
    /// missing or unreadable stamp allows the check rather than blocking
    /// updates forever.
    fn is_check_due(&self) -> bool {
        match fs::read_to_string(&self.last_check_file) {
            Ok(content) => {
                let last_check = content
                    .parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now() - Duration::days(DAILY_CHECK_INTERVAL + 1));

                Utc::now().signed_duration_since(last_check) > Duration::days(DAILY_CHECK_INTERVAL)
            }
            Err(_) => true,
        }
    }
}
