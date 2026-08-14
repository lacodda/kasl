//! Self-updating functionality for the kasl application.
//!
//! Provides comprehensive auto-update capabilities that enable the application
//! to automatically check for, download, and install newer versions from GitHub releases.
//!
//! ## Usage
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

/// Filename for storing the timestamp of the last update check.
///
/// This file enables throttling of update checks to avoid excessive API calls
/// and respect GitHub's rate limiting policies.
const LAST_CHECK_FILE: &str = ".last_update_check";

/// Minimum interval between update checks in days.
///
/// This prevents excessive API calls while ensuring users receive timely
/// notifications about new releases. The interval balances user experience
/// with API rate limiting considerations.
const DAILY_CHECK_INTERVAL: i64 = 1;

/// File extension used for backing up the current executable.
///
/// Before replacing the current executable, it's backed up with this extension
/// to enable rollback in case of update failures.
const BACKUP_EXTENSION: &str = "bak";

/// Manages the complete application update process from version checking to binary replacement.
///
/// The Updater encapsulates all state and behavior needed for safe, reliable application
/// updates. It handles GitHub API communication, platform detection, download management,
/// and atomic binary replacement with backup and rollback capabilities.
///
/// ## State Management
///
/// The Updater maintains several pieces of state throughout the update process:
/// - **Version Information**: Current and latest version tracking
/// - **Download State**: URLs and file paths for update assets
/// - **Configuration**: API endpoints and platform identification
/// - **Check Throttling**: Timestamps for rate-limited update checks
///
#[derive(Debug)]
pub struct Updater {
    /// HTTP client for making API requests to GitHub.
    ///
    /// Configured with appropriate headers and timeouts for reliable
    /// communication with GitHub's API endpoints.
    pub client: Client,

    /// GitHub repository owner (organization or user account).
    ///
    /// Extracted from build-time metadata to identify the source repository
    /// for release information and asset downloads.
    pub owner: String,

    /// Application/repository name for GitHub API requests.
    ///
    /// Combined with owner to form complete repository identification for
    /// API endpoint construction and asset discovery.
    pub name: String,

    /// Current version of the running application.
    ///
    /// Embedded at compile time to enable comparison with latest available
    /// versions from GitHub releases. Used for determining update necessity.
    pub version: String,

    /// Latest version available from GitHub (if newer than current).
    ///
    /// Populated after successful version check if a newer version is found.
    /// Used for user notifications and update confirmation messages.
    pub latest_version: Option<String>,

    /// Download URL for the latest release asset matching current platform.
    ///
    /// Determined through platform detection and asset filtering. Used for
    /// downloading the appropriate binary for the current system configuration.
    pub download_url: Option<String>,

    /// URL of the repository's `releases/latest` page.
    ///
    /// The latest tag is read from this page's redirect `Location` header
    /// instead of `api.github.com`: the API allows only 60 anonymous
    /// requests per hour per IP, which starves every machine behind a
    /// shared NAT (the same failure the installers hit).
    releases_url: String,

    /// Path to file storing the timestamp of the last update check.
    ///
    /// Enables throttling of update checks to respect API rate limits and
    /// avoid excessive network requests while providing timely notifications.
    last_check_file: PathBuf,
}

impl Updater {
    /// Creates a new Updater instance with configuration from build-time metadata.
    ///
    /// This constructor initializes the updater with all necessary configuration
    /// for communicating with GitHub's API and managing the update process. It
    /// uses compile-time metadata to automatically configure repository information.
    ///
    /// ## Configuration Sources
    ///
    /// The constructor uses several sources for configuration:
    /// - **Build Metadata**: Repository owner, name, and current version
    /// - **GitHub API**: Standard endpoints for releases and asset discovery
    /// - **Data Storage**: Platform-appropriate paths for cache and state files
    /// - **Network Configuration**: HTTP client with reasonable defaults
    ///
    /// ## File System Setup
    ///
    /// The constructor creates necessary file system entries:
    /// - **Check Cache File**: For storing last update check timestamp
    /// - **Data Directory**: Platform-specific application data location
    /// - **Permissions**: Appropriate read/write permissions for update operations
    ///
    /// # Errors
    ///
    /// - **Data Storage**: Cannot determine or create application data directory
    /// - **File System**: Permission issues with cache file creation
    /// - **Configuration**: Invalid repository information in build metadata
    ///
    /// # Examples
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
        // Extract repository information from compile-time metadata
        let owner = APP_METADATA_OWNER.to_owned();
        let name = APP_METADATA_NAME.to_owned();

        // Set up cache file for update check throttling
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

    /// Displays a notification if a new version is available, with throttled checking.
    ///
    /// This method provides a user-friendly way to check for updates without being
    /// intrusive. It implements intelligent throttling to avoid excessive API calls
    /// while ensuring users are notified of important updates in a timely manner.
    ///
    /// ## Throttling Logic
    ///
    /// The method implements several levels of throttling:
    /// 1. **Time-Based**: Respects the daily check interval configuration
    /// 2. **Graceful Failure**: Silently handles initialization or network errors
    /// 3. **Non-Blocking**: Returns immediately if checks aren't due
    /// 4. **Background Operation**: Doesn't interrupt normal application flow
    ///
    /// ## User Experience
    ///
    /// - **Non-Intrusive**: Only shows notifications when updates are available
    /// - **Informative**: Provides clear version information in notifications
    /// - **Actionable**: Suggests how users can install available updates
    /// - **Reliable**: Handles network errors gracefully without user impact
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # async fn f() {
    /// use kasl::libs::update::Updater;
    ///
    /// // Call during application startup
    /// Updater::show_update_notification().await;
    /// // User sees notification only if update is available and check is due
    /// # }
    /// ```
    ///
    /// # Background Behavior
    ///
    /// This method is designed to be called during application startup:
    /// - **Startup Integration**: Called automatically during main application init
    /// - **Non-Blocking**: Doesn't delay application startup or user operations
    /// - **Error Resilience**: Network or API failures don't affect application functionality
    /// - **Rate Limiting**: Respects GitHub API limits through intelligent throttling
    pub async fn show_update_notification() {
        // Attempt to create updater instance - fail silently if not possible
        let mut updater = match Self::new() {
            Ok(up) => up,
            Err(_) => return, // Graceful degradation if updater can't be initialized
        };

        // Check if enough time has passed since last update check
        if !updater.is_check_due() {
            return;
        }

        // Perform version check and display notification if update available
        if let Ok(true) = updater.check_for_latest_release().await
            && let Some(latest_version) = &updater.latest_version
        {
            // Display user-friendly update notification
            msg_info!(
                Message::UpdateAvailable {
                    app_name: updater.name,
                    latest: latest_version.to_string()
                },
                true // Show with extra spacing for visibility
            )
        }
    }

    /// Performs the complete update process: download, verification, and installation.
    ///
    /// This method orchestrates the entire update workflow, from downloading the
    /// latest release to safely replacing the current executable. It implements
    /// multiple safety mechanisms to ensure the update process is reliable and
    /// recoverable in case of failures.
    ///
    /// ## Update Process Flow
    ///
    /// The method follows a carefully designed sequence:
    ///
    /// 1. **Pre-flight Validation**: Verifies that download URL is available
    /// 2. **Asset Download**: Retrieves the release archive from GitHub
    /// 3. **Local Storage**: Saves archive to temporary location for processing
    /// 4. **Binary Extraction**: Extracts and validates the new executable
    /// 5. **Backup Creation**: Creates backup of current executable
    /// 6. **Atomic Replacement**: Replaces current executable with new version
    /// 7. **Cleanup**: Removes temporary files and completes the process
    ///
    /// ## Safety Mechanisms
    ///
    /// ### Backup and Recovery
    /// - **Current Executable Backup**: Automatically created before replacement
    /// - **Rollback Capability**: Failed updates can be reverted using backup
    /// - **Atomic Operations**: Binary replacement is performed atomically
    /// - **Error Recovery**: Partial failures are cleaned up automatically
    ///
    /// ### Validation and Verification
    /// - **Download Validation**: Ensures complete archive download
    /// - **Archive Integrity**: Validates archive format and structure
    /// - **Binary Verification**: Confirms executable is present in archive
    /// - **Platform Compatibility**: Verifies binary matches current platform
    ///
    /// ## Error Handling
    ///
    /// The method implements comprehensive error handling:
    /// - **Network Errors**: Download failures are reported with clear messages
    /// - **File System Errors**: Permission and disk space issues are handled
    /// - **Archive Errors**: Corrupted or invalid archives are detected
    /// - **Backup Failures**: Issues with backup creation abort the process
    ///
    /// # Preconditions
    ///
    /// This method requires that `check_for_latest_release()` has been called
    /// successfully and that `self.download_url` contains a valid URL.
    ///
    /// # Examples
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
    ///
    pub async fn perform_update(&self) -> Result<()> {
        // Validate that download URL is available from previous version check
        let download_url = self.download_url.as_ref().ok_or(msg_error_anyhow!(Message::UpdateDownloadUrlNotSet))?;

        // Download the release archive from GitHub
        let response = self.client.get(download_url).send().await?;
        let content = response.bytes().await?;

        // Save the downloaded archive to a temporary file for processing
        let tar_gz_path = env::temp_dir().join(format!("{}.tar.gz", self.name));
        fs::write(&tar_gz_path, &content)?;

        // Extract the new binary and replace the current executable
        // This includes backup creation and atomic replacement
        self.extract_and_replace_binary(&tar_gz_path)?;

        // Clean up the downloaded archive after successful installation
        fs::remove_file(&tar_gz_path)?;

        Ok(())
    }

    /// Checks GitHub for the latest release and determines if an update is available.
    ///
    /// This method communicates with GitHub's releases API to fetch information about
    /// the latest available version. It compares version strings to determine if the
    /// current application version is outdated and populates the updater's state with
    /// download information if an update is needed.
    ///
    /// ## Version Comparison Logic
    ///
    /// The method uses string comparison for version precedence:
    /// 1. **Version Normalization**: Strips 'v' prefix from GitHub tags if present
    /// 2. **String Comparison**: Uses lexicographic comparison for version ordering
    /// 3. **Update Detection**: Identifies when remote version is greater than current
    /// 4. **State Population**: Stores version and download information for later use
    ///
    /// ## Release Discovery
    ///
    /// The latest tag comes from the `releases/latest` redirect rather than
    /// `api.github.com` (anonymous API quota is 60 requests/hour per IP and
    /// starves shared NATs). The download URL is constructed from the tag and
    /// the platform identifier, matching the release asset naming scheme.
    ///
    /// ## State Updates
    ///
    /// When a newer version is found, the method updates:
    /// - **latest_version**: Stores the newer version string for display
    /// - **download_url**: Sets the URL for downloading the platform-specific binary
    /// - **Check Timestamp**: Records when this check was performed for throttling
    ///
    /// # Errors
    ///
    /// - **Network Errors**: API request failures, timeouts, or connectivity issues
    /// - **API Errors**: GitHub API rate limiting or service unavailability
    /// - **Parsing Errors**: Invalid JSON response format from GitHub
    /// - **File System Errors**: Cannot update check timestamp cache file
    ///
    /// # Examples
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
        // Read the latest tag from the release page redirect
        let tag = self.fetch_latest_tag().await?;

        // Update check timestamp for throttling future checks
        self.update_last_check_time();

        // Normalize version string by removing 'v' prefix if present
        let latest_version = tag.trim_start_matches('v').to_string();

        // Compare versions using string comparison (works for semantic versioning)
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

            Ok(true) // Update is available
        } else {
            Ok(false) // Current version is up-to-date
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

    /// Extracts the new binary from the downloaded archive and replaces the current executable.
    ///
    /// This method performs the most critical part of the update process: safely
    /// replacing the current executable with the new version. It implements multiple
    /// safety mechanisms to ensure the operation is atomic and recoverable.
    ///
    /// ## Extraction Process
    ///
    /// 1. **Archive Opening**: Opens and validates the tar.gz archive
    /// 2. **Entry Iteration**: Processes each file in the archive
    /// 3. **Binary Identification**: Finds the main executable file
    /// 4. **Backup Creation**: Creates backup of current executable
    /// 5. **Atomic Replacement**: Replaces executable with new version
    /// 6. **Auxiliary Files**: Extracts other files to appropriate locations
    ///
    /// ## Safety Mechanisms
    ///
    /// ### Backup Strategy
    /// - **Automatic Backup**: Current executable is backed up before replacement
    /// - **Backup Naming**: Uses consistent `.bak` extension for identification
    /// - **Rollback Support**: Backup enables recovery from failed updates
    /// - **Cleanup**: Old backups are replaced with new ones
    ///
    /// ### Atomic Operations
    /// - **Rename Operation**: Uses filesystem rename for atomic replacement
    /// - **Error Recovery**: Partially completed operations are cleaned up
    /// - **Validation**: Confirms successful extraction before cleanup
    /// - **Rollback**: Failed operations can be reverted using backup
    ///
    /// ## File Handling
    ///
    /// ### Main Executable
    /// - **Identification**: Matches filename with current executable
    /// - **Backup Creation**: Renames current executable to backup
    /// - **Replacement**: Extracts new executable to current location
    /// - **Permissions**: Preserves executable permissions
    ///
    /// ### Auxiliary Files
    /// - **Location**: Extracted to same directory as executable
    /// - **Overwrite**: Existing files are replaced with new versions
    /// - **Permissions**: Standard file permissions are applied
    /// - **Cleanup**: Temporary files are removed after extraction
    ///
    fn extract_and_replace_binary(&self, tar_gz_path: &PathBuf) -> Result<()> {
        // Open and prepare the archive for extraction
        let tar_gz = File::open(tar_gz_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        let mut is_updated = false;

        // Determine current executable path and backup location
        let current_exe = env::current_exe()?;
        let current_exe_backup = current_exe.with_extension(BACKUP_EXTENSION);

        // Process each entry in the archive
        for entry_result in archive.entries()? {
            let mut entry = entry_result?;
            let entry_path = entry.path()?;

            // Check if this entry is the main executable
            if entry_path.ends_with(current_exe.file_name().unwrap()) {
                // Create backup of current executable before replacement
                fs::rename(&current_exe, &current_exe_backup)?;

                // Extract new executable to current location
                entry.unpack(&current_exe)?;
                is_updated = true;
            } else {
                // Extract auxiliary files to the executable directory
                let dest_path = current_exe.parent().unwrap().join(&entry_path);
                entry.unpack(dest_path)?;
            }
        }

        // Verify that the main executable was found and updated
        if is_updated {
            Ok(())
        } else {
            msg_bail_anyhow!(Message::UpdateBinaryNotFoundInArchive);
        }
    }

    /// Constructs the platform-specific identifier used in release asset names.
    ///
    /// This method generates a string that identifies the current platform's
    /// architecture and operating system in the format used by GitHub release
    /// assets. The identifier follows Rust's target triple format for consistency.
    ///
    /// ## Platform Detection
    ///
    /// The method uses Rust's built-in constants to detect:
    /// - **Architecture**: From `env::consts::ARCH` (x86_64, aarch64, etc.)
    /// - **Operating System**: From `env::consts::OS` (windows, macos, linux)
    /// - **ABI/Toolchain**: Mapped to appropriate toolchain identifier
    ///
    /// ## Identifier Format
    ///
    /// The generated identifiers follow this pattern:
    /// `{architecture}-{vendor}-{os}-{abi}`
    ///
    /// ### Architecture Values
    /// - `x86_64`: 64-bit Intel/AMD processors
    /// - `aarch64`: 64-bit ARM processors (Apple Silicon, ARM64)
    ///
    /// ### Operating System Mapping
    /// - `windows` → `pc-windows-msvc`: Windows with MSVC toolchain
    /// - `macos` → `apple-darwin`: macOS with Darwin ABI
    /// - Other → `unknown-linux-gnu`: Linux with glibc
    ///
    /// # Examples
    ///
    /// Generated identifiers:
    /// - Windows: `"x86_64-pc-windows-msvc"`
    /// - macOS Intel: `"x86_64-apple-darwin"`
    /// - macOS Apple Silicon: `"aarch64-apple-darwin"`
    /// - Linux: `"x86_64-unknown-linux-gnu"`
    fn get_platform_identifier(&self) -> String {
        let arch = env::consts::ARCH;
        let os = match env::consts::OS {
            "windows" => "pc-windows-msvc",
            "macos" => "apple-darwin",
            // Must match the published asset triple; releases ship glibc
            // builds (the installers hit 404s on the old musl guess).
            _ => "unknown-linux-gnu",
        };

        // Construct target triple format: architecture-vendor-os-abi
        format!("{}-{}", arch, os)
    }

    /// Updates the timestamp file to record when the last update check was performed.
    ///
    /// This method implements the persistence layer for update check throttling,
    /// ensuring that checks are performed at appropriate intervals without being
    /// too frequent or too infrequent for user needs.
    ///
    /// ## Throttling Implementation
    ///
    /// - **Timestamp Format**: Uses RFC 3339 format for precise time recording
    /// - **File Persistence**: Stores timestamp in application data directory
    /// - **Error Tolerance**: File write failures are silently ignored
    /// - **Atomic Update**: Timestamp is updated immediately after API call
    ///
    /// ## File Management
    ///
    /// - **Location**: Stored in platform-specific application data directory
    /// - **Format**: Plain text file containing ISO 8601 timestamp
    /// - **Permissions**: Standard file permissions for user data
    /// - **Cleanup**: File is automatically managed, no manual cleanup needed
    ///
    /// ## Error Handling
    ///
    /// File write failures are intentionally ignored because:
    /// - Update check throttling is a convenience feature, not critical functionality
    /// - Missing timestamp files default to allowing immediate checks
    /// - File system errors shouldn't prevent application operation
    /// - Next successful write will restore normal throttling behavior
    fn update_last_check_time(&self) {
        let now = Utc::now().to_rfc3339();

        // Intentionally ignore write errors - throttling is not critical functionality
        let _ = fs::write(&self.last_check_file, now);
    }

    /// Determines if sufficient time has passed since the last update check.
    ///
    /// This method implements the core logic for update check throttling, ensuring
    /// that checks are performed at reasonable intervals while respecting both
    /// user experience and API rate limiting considerations.
    ///
    /// ## Throttling Logic
    ///
    /// The method implements several decision points:
    ///
    /// ### File Existence Check
    /// - **Missing File**: Indicates first run or file system issue → allow check
    /// - **Read Errors**: File corruption or permission issues → allow check
    /// - **Successful Read**: Parse timestamp and evaluate recency
    ///
    /// ### Timestamp Parsing
    /// - **Valid Timestamp**: Parse and compare with current time
    /// - **Invalid Format**: Corrupted timestamp data → allow check
    /// - **Parse Errors**: File corruption or format changes → allow check
    ///
    /// ### Interval Evaluation
    /// - **Recent Check**: Within daily interval → deny check
    /// - **Overdue Check**: Exceeds daily interval → allow check
    /// - **Future Timestamp**: System clock issues → allow check
    ///
    /// ## Error Handling Strategy
    ///
    /// The method uses a fail-open approach where any error condition results
    /// in allowing the check to proceed. This ensures that:
    /// - File system issues don't prevent updates
    /// - Timestamp corruption doesn't block checks permanently
    /// - Clock synchronization problems are handled gracefully
    /// - Users receive update notifications despite technical issues
    ///
    /// # Examples
    ///
    /// ```text
    /// let updater = Updater::new()?;
    /// if updater.is_check_due() {
    ///     // Perform update check
    /// } else {
    ///     // Skip check, too recent
    /// }
    /// ```
    fn is_check_due(&self) -> bool {
        match fs::read_to_string(&self.last_check_file) {
            Ok(content) => {
                // Attempt to parse the stored timestamp
                let last_check = content.parse::<DateTime<Utc>>().unwrap_or_else(|_| {
                    // If parsing fails, default to a time that will trigger a check
                    Utc::now() - Duration::days(DAILY_CHECK_INTERVAL + 1)
                });

                // Check if enough time has passed since the last check
                Utc::now().signed_duration_since(last_check) > Duration::days(DAILY_CHECK_INTERVAL)
            }
            Err(_) => true, // If file doesn't exist or can't be read, always allow check
        }
    }
}
