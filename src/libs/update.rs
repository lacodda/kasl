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
use std::path::{Path, PathBuf};
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

    /// Unpacks the release archive over the installed binaries.
    ///
    /// Only the executables are taken: `kasl` (renamed to `.bak` first, so a
    /// broken update can be undone by hand) and, when the alias sits next to
    /// it, `ka`. LICENSE and README are skipped - copying them used to
    /// recreate the archive's `kasl-<tag>-<target>/` prefix inside the
    /// installation directory, leaving a folder of stale duplicates behind
    /// after every update.
    fn extract_and_replace_binary(&self, tar_gz_path: &PathBuf) -> Result<()> {
        let current_exe = env::current_exe()?;
        let install_dir = current_exe.parent().unwrap().to_path_buf();

        Self::unpack_binaries(tar_gz_path, &install_dir, &self.name)
    }

    /// Replaces the binaries in `install_dir` from the archive.
    ///
    /// Split out from [`Updater::extract_and_replace_binary`] so the layout
    /// rules can be tested against a real archive without a real update:
    /// both release bugs found in the field (the alias missing, the leftover
    /// version folders) lived here, untested.
    pub(crate) fn unpack_binaries(tar_gz_path: &PathBuf, install_dir: &Path, app_name: &str) -> Result<()> {
        // The app updates under its own name, not under whichever name was
        // typed: `ka update` must still replace `kasl`.
        let exe_suffix = env::consts::EXE_SUFFIX;
        let primary = format!("{}{}", app_name, exe_suffix);
        let alias = format!("ka{}", exe_suffix);

        let tar_gz = File::open(tar_gz_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        let mut is_updated = false;

        for entry_result in archive.entries()? {
            let mut entry = entry_result?;
            let entry_path = entry.path()?.to_path_buf();
            let Some(file_name) = entry_path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            // Flattened on purpose: archive entries carry a
            // `kasl-<tag>-<target>/` prefix that must not reach the
            // installation directory.
            if file_name == primary {
                let target = install_dir.join(&primary);
                // Keep the replaced binary as the one-and-only backup.
                if target.exists() {
                    fs::rename(&target, target.with_extension(BACKUP_EXTENSION))?;
                }
                entry.unpack(&target)?;
                is_updated = true;
            } else if file_name == alias {
                let target = install_dir.join(&alias);
                // The alias is refreshed only where it is already installed:
                // updating must not add a binary the user declined
                // (`KASL_NO_ALIAS`), but a `ka` left behind at an older
                // version would be worse than none at all.
                if target.exists() {
                    fs::remove_file(&target)?;
                    entry.unpack(&target)?;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tempfile::TempDir;

    /// Builds an archive shaped like a real release asset: every entry sits
    /// under a `kasl-<tag>-<target>/` directory, next to LICENSE and README.
    fn release_archive(dir: &Path, files: &[(&str, &str)]) -> PathBuf {
        let path = dir.join("release.tar.gz");
        let encoder = GzEncoder::new(File::create(&path).unwrap(), Compression::default());
        let mut builder = tar::Builder::new(encoder);

        for (name, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, format!("kasl-v9.9.9-x86_64-pc-windows-msvc/{name}"), contents.as_bytes())
                .unwrap();
        }

        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    fn exe(name: &str) -> String {
        format!("{}{}", name, env::consts::EXE_SUFFIX)
    }

    #[test]
    fn the_archive_directory_prefix_stays_out_of_the_installation() {
        // Field report, 14.08: every update left a `kasl-v1.2.0/` folder with
        // copies of LICENSE and README next to the binary, because non-binary
        // entries were unpacked under their in-archive path.
        let temp = TempDir::new().unwrap();
        let install = temp.path().join("install");
        fs::create_dir(&install).unwrap();
        fs::write(install.join(exe("kasl")), "old").unwrap();

        let archive = release_archive(temp.path(), &[(&exe("kasl"), "new"), ("LICENSE", "MIT"), ("README.md", "docs")]);

        Updater::unpack_binaries(&archive, &install, "kasl").unwrap();

        let leftovers: Vec<_> = fs::read_dir(&install)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("kasl-v"))
            .collect();
        assert!(leftovers.is_empty(), "update left {leftovers:?} in the installation directory");
        assert!(!install.join("LICENSE").exists(), "LICENSE does not belong next to the binary");
        assert!(!install.join("README.md").exists(), "README does not belong next to the binary");
    }

    #[test]
    fn the_binary_is_replaced_and_the_old_one_kept_as_backup() {
        let temp = TempDir::new().unwrap();
        let install = temp.path().join("install");
        fs::create_dir(&install).unwrap();
        fs::write(install.join(exe("kasl")), "old").unwrap();

        let archive = release_archive(temp.path(), &[(&exe("kasl"), "new")]);
        Updater::unpack_binaries(&archive, &install, "kasl").unwrap();

        assert_eq!(fs::read_to_string(install.join(exe("kasl"))).unwrap(), "new");
        assert_eq!(
            fs::read_to_string(install.join("kasl.bak")).unwrap(),
            "old",
            "the replaced binary must remain recoverable"
        );
    }

    #[test]
    fn an_installed_alias_is_updated_together_with_the_binary() {
        // A `ka` left at the previous version is a trap: it answers to the
        // same commands while running older code.
        let temp = TempDir::new().unwrap();
        let install = temp.path().join("install");
        fs::create_dir(&install).unwrap();
        fs::write(install.join(exe("kasl")), "old").unwrap();
        fs::write(install.join(exe("ka")), "old").unwrap();

        let archive = release_archive(temp.path(), &[(&exe("kasl"), "new"), (&exe("ka"), "new")]);
        Updater::unpack_binaries(&archive, &install, "kasl").unwrap();

        assert_eq!(fs::read_to_string(install.join(exe("ka"))).unwrap(), "new");
    }

    #[test]
    fn an_absent_alias_is_not_installed_by_an_update() {
        // `KASL_NO_ALIAS=1` at install time is a choice; an update must not
        // quietly overturn it.
        let temp = TempDir::new().unwrap();
        let install = temp.path().join("install");
        fs::create_dir(&install).unwrap();
        fs::write(install.join(exe("kasl")), "old").unwrap();

        let archive = release_archive(temp.path(), &[(&exe("kasl"), "new"), (&exe("ka"), "new")]);
        Updater::unpack_binaries(&archive, &install, "kasl").unwrap();

        assert!(!install.join(exe("ka")).exists(), "the update added an alias the user never installed");
    }

    #[test]
    fn an_archive_without_the_binary_fails_instead_of_reporting_success() {
        let temp = TempDir::new().unwrap();
        let install = temp.path().join("install");
        fs::create_dir(&install).unwrap();

        let archive = release_archive(temp.path(), &[("LICENSE", "MIT")]);
        assert!(Updater::unpack_binaries(&archive, &install, "kasl").is_err());
    }
}
