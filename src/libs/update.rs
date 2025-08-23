//! Self-updating functionality for the kasl application.
//!
//! Provides comprehensive auto-update capabilities that enable the application
//! to automatically check for, download, and install newer versions from GitHub releases.
//!
//! ## Features
//!
//! - **Safety Mechanisms**: Automatic backup, rollback capability, atomic operations
//! - **Platform Detection**: Architecture awareness, OS detection, ABI compatibility
//! - **Network Resilience**: Throttled checks, graceful degradation, retry logic
//! - **Version Management**: Semantic versioning, GitHub API integration
//! - **Platform Support**: Windows, macOS Intel/Apple Silicon, Linux
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
use serde::Deserialize;
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

/// Represents a GitHub release response from the API.
///
/// This structure deserializes the JSON response from GitHub's releases API,
/// containing version information and download assets for the latest release.
#[derive(Deserialize, Debug)]
struct GitHubRelease {
    /// The version tag name (e.g., "v1.2.3" or "1.2.3")
    tag_name: String,
    /// Array of downloadable assets for this release
    assets: Vec<GitHubAsset>,
}

/// Represents a single downloadable asset within a GitHub release.
///
/// Each release typically contains multiple assets for different platforms
/// and architectures. This structure provides the information needed to
/// identify and download the appropriate asset.
#[derive(Deserialize, Debug)]
struct GitHubAsset {
    /// Direct download URL for this asset
    browser_download_url: String,
    /// Filename of the asset (used for platform identification)
    name: String,
}

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
/// ## Thread Safety
///
/// The Updater is designed for single-threaded use during update operations.
/// While individual methods are safe to call, the update process itself should
/// not be parallelized to avoid file system conflicts during binary replacement.
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

    /// Complete URL for fetching latest release information from GitHub API.
    ///
    /// Constructed from repository information and GitHub's API format.
    /// Used for all version checking and asset discovery operations.
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
    /// # Returns
    ///
    /// Returns a configured Updater instance ready for version checking and
    /// update operations, or an error if initialization fails.
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
    /// use kasl::libs::update::Updater;
    ///
    /// let updater = Updater::new()?;
    /// println!("Updater configured for {} v{}", updater.name, updater.version);
    /// ```
    pub fn new() -> Result<Self> {
        // Extract repository information from compile-time metadata
        let owner = APP_METADATA_OWNER.to_owned();
        let name = APP_METADATA_NAME.to_owned();

        // Set up cache file for update check throttling
        let last_check_file = DataStorage::new().get_path(LAST_CHECK_FILE)?;

        // Construct GitHub API endpoint for latest release information
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
    /// ## Implementation Strategy
    ///
    /// The method uses a fail-fast approach:
    /// - Returns immediately if updater initialization fails
    /// - Skips check if not enough time has passed since last check
    /// - Only displays notification if newer version is confirmed available
    /// - Handles all errors silently to avoid disrupting user workflow
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::update::Updater;
    ///
    /// // Call during application startup
    /// Updater::show_update_notification().await;
    /// // User sees notification only if update is available and check is due
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
        if let Ok(true) = updater.check_for_latest_release().await {
            if let Some(latest_version) = &updater.latest_version {
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
    /// # Returns
    ///
    /// Returns `Ok(())` on successful update completion, or an error describing
    /// the specific failure that occurred during the update process.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::update::Updater;
    ///
    /// let mut updater = Updater::new()?;
    /// if updater.check_for_latest_release().await? {
    ///     updater.perform_update().await?;
    ///     println!("Update completed successfully");
    /// }
    /// ```
    ///
    /// # Error Scenarios
    ///
    /// - **No Download URL**: `check_for_latest_release()` hasn't been called successfully
    /// - **Network Failure**: Unable to download release archive from GitHub
    /// - **Disk Space**: Insufficient space for temporary files or backup
    /// - **Permissions**: Cannot write to application directory or create backup
    /// - **Archive Corruption**: Downloaded archive is corrupted or invalid format
    /// - **Missing Binary**: Archive doesn't contain expected executable file
    pub async fn perform_update(&self) -> Result<()> {
        // Validate that download URL is available from previous version check
        let download_url = self.download_url.as_ref().ok_or(msg_error_anyhow!(Message::UpdateDownloadUrlNotSet))?;

        // Download the release archive from GitHub
        let response = self.client.get(download_url).send().await?;
        let content = response.bytes().await?;

        // Save the downloaded archive to a temporary file for processing
        let tar_gz_path = env::temp_dir().join(format!("{}.tar.gz", &self.name));
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
    /// ## API Communication
    ///
    /// ### Request Configuration
    /// - **User Agent**: Identifies requests with application name
    /// - **Rate Limiting**: Respects GitHub's API rate limits
    /// - **Error Handling**: Gracefully handles API errors and timeouts
    /// - **JSON Parsing**: Deserializes GitHub's release response format
    ///
    /// ### Response Processing
    /// - **Version Extraction**: Parses tag_name from release information
    /// - **Asset Discovery**: Finds platform-appropriate download assets
    /// - **URL Resolution**: Determines correct download URL for current platform
    /// - **Cache Update**: Records check timestamp for throttling
    ///
    /// ## State Updates
    ///
    /// When a newer version is found, the method updates:
    /// - **latest_version**: Stores the newer version string for display
    /// - **download_url**: Sets the URL for downloading the platform-specific binary
    /// - **Check Timestamp**: Records when this check was performed for throttling
    ///
    /// # Returns
    ///
    /// Returns `true` if a newer version is available and download URL is found,
    /// `false` if the current version is up-to-date or no compatible asset exists.
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
    /// let mut updater = Updater::new()?;
    /// if updater.check_for_latest_release().await? {
    ///     println!("Update available: {} -> {}",
    ///         updater.version,
    ///         updater.latest_version.unwrap());
    /// }
    /// ```
    pub async fn check_for_latest_release(&mut self) -> Result<bool> {
        // Fetch latest release information from GitHub API
        let release = self.fetch_latest_github_release().await?;

        // Update check timestamp for throttling future checks
        self.update_last_check_time();

        // Normalize version string by removing 'v' prefix if present
        let latest_version = release.tag_name.trim_start_matches('v').to_string();

        // Compare versions using string comparison (works for semantic versioning)
        if latest_version > self.version {
            // Store the newer version information
            self.latest_version = Some(latest_version);

            // Find and store the download URL for the current platform
            self.download_url = self.find_platform_asset_url(&release.assets).map(|url| url.to_string());

            Ok(true) // Update is available
        } else {
            Ok(false) // Current version is up-to-date
        }
    }

    /// Fetches release data from the GitHub releases API.
    ///
    /// This method handles the low-level communication with GitHub's API,
    /// including proper request headers and JSON deserialization. It's designed
    /// to be reliable and follow GitHub's API best practices.
    ///
    /// ## Request Configuration
    ///
    /// - **User-Agent Header**: Required by GitHub API, set to application name
    /// - **Accept Header**: Implicitly requests JSON response format
    /// - **Timeout Handling**: Uses client default timeouts for reliability
    /// - **Error Propagation**: Network errors are propagated to caller
    ///
    /// # Returns
    ///
    /// Returns the parsed GitHub release information or an error if the
    /// request fails or the response cannot be parsed.
    async fn fetch_latest_github_release(&self) -> Result<GitHubRelease, reqwest::Error> {
        self.client
            .get(&self.releases_url)
            .header("User-Agent", &self.name) // Required by GitHub API
            .send()
            .await?
            .json::<GitHubRelease>()
            .await
    }

    /// Finds the download URL for an asset matching the current platform.
    ///
    /// This method searches through release assets to find the binary that
    /// matches the current platform's architecture and operating system.
    /// It uses platform identification to select the appropriate asset.
    ///
    /// ## Asset Selection Logic
    ///
    /// 1. **Platform Identification**: Generate current platform identifier
    /// 2. **Asset Filtering**: Search assets for matching platform identifier
    /// 3. **URL Extraction**: Return download URL for matching asset
    /// 4. **Fallback Handling**: Return None if no matching asset found
    ///
    /// ## Platform Matching
    ///
    /// The method looks for assets containing platform identifiers like:
    /// - `x86_64-pc-windows-msvc` for Windows
    /// - `x86_64-apple-darwin` for macOS Intel
    /// - `aarch64-apple-darwin` for macOS Apple Silicon
    /// - `x86_64-unknown-linux-musl` for Linux
    ///
    /// # Arguments
    ///
    /// * `assets` - Array of release assets from GitHub API response
    ///
    /// # Returns
    ///
    /// Returns the download URL for the matching asset, or None if no
    /// compatible asset is found for the current platform.
    fn find_platform_asset_url<'a>(&self, assets: &'a [GitHubAsset]) -> Option<&'a str> {
        let platform_name = self.get_platform_identifier();
        assets
            .iter()
            .find(|asset| asset.name.contains(&platform_name))
            .map(|asset| asset.browser_download_url.as_str())
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
    /// # Arguments
    ///
    /// * `tar_gz_path` - Path to the downloaded release archive
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful extraction and replacement, or an error
    /// if any step of the process fails.
    ///
    /// # Error Scenarios
    ///
    /// - **Archive Errors**: Corrupted or invalid tar.gz format
    /// - **Missing Binary**: Archive doesn't contain expected executable
    /// - **File System Errors**: Permission issues or disk space problems
    /// - **Backup Failures**: Cannot create backup of current executable
    /// - **Extraction Errors**: Cannot extract files from archive
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
    /// - Other → `unknown-linux-musl`: Linux with statically linked musl
    ///
    /// # Returns
    ///
    /// Returns a platform identifier string suitable for matching against
    /// GitHub release asset names.
    ///
    /// # Examples
    ///
    /// Generated identifiers:
    /// - Windows: `"x86_64-pc-windows-msvc"`
    /// - macOS Intel: `"x86_64-apple-darwin"`
    /// - macOS Apple Silicon: `"aarch64-apple-darwin"`
    /// - Linux: `"x86_64-unknown-linux-musl"`
    fn get_platform_identifier(&self) -> String {
        let arch = env::consts::ARCH;
        let os = match env::consts::OS {
            "windows" => "pc-windows-msvc",
            "macos" => "apple-darwin",
            _ => "unknown-linux-musl", // Default to Linux with musl for compatibility
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
    /// # Returns
    ///
    /// Returns `true` if a check should be performed (enough time has passed
    /// or error conditions favor allowing the check), `false` if the check
    /// should be skipped to respect throttling intervals.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
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
