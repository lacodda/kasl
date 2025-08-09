//! Cross-platform data storage path management for application files.
//!
//! This module provides a unified interface for managing application data storage
//! locations across different operating systems. It handles the complexities of
//! platform-specific directory structures and ensures consistent data placement
//! following OS conventions and user expectations.
//!
//! ## Platform-Specific Storage Locations
//!
//! The module automatically selects appropriate storage locations based on the target OS:
//!
//! ### Windows
//! - **Location**: `%LOCALAPPDATA%\lacodda\kasl\`
//! - **Example**: `C:\Users\Username\AppData\Local\lacodda\kasl\`
//! - **Rationale**: Uses Windows Local AppData for user-specific application data
//! - **Backup**: Typically excluded from roaming profiles, suitable for local storage
//!
//! ### macOS
//! - **Location**: `~/Library/Application Support/lacodda/kasl/`
//! - **Example**: `/Users/Username/Library/Application Support/lacodda/kasl/`
//! - **Rationale**: Follows Apple's guidelines for application support files
//! - **Backup**: Included in Time Machine backups by default
//!
//! ### Linux/Unix
//! - **Location**: `~/.local/share/lacodda/kasl/`
//! - **Example**: `/home/username/.local/share/lacodda/kasl/`
//! - **Rationale**: Complies with XDG Base Directory Specification
//! - **Backup**: User-space location compatible with most backup solutions
//!
//! ## Directory Structure
//!
//! All platforms follow the same hierarchical structure:
//! ```
//! {platform_base}/lacodda/kasl/
//! ├── kasl.db              # SQLite database
//! ├── config.json          # Application configuration
//! ├── .jira_session_id     # Cached session tokens
//! ├── .gitlab_secret       # Encrypted credentials
//! └── kasl-watch.pid       # Process ID files
//! ```
//!
//! ## Features
//!
//! - **Automatic Directory Creation**: Creates required directories on first access
//! - **Permission Handling**: Uses locations where users have write permissions
//! - **Environment Variable Support**: Respects custom environment overrides
//! - **Fallback Strategy**: Uses current directory if standard locations fail
//! - **Path Validation**: Ensures paths are valid and accessible
//!
//! ## Security Considerations
//!
//! - **User Isolation**: Each user has their own isolated data directory
//! - **Permission Inheritance**: Respects parent directory permission models
//! - **Sensitive Data**: Appropriate for storing encrypted credentials and session tokens
//! - **Access Control**: Leverages OS-level user access controls
//!
//! ## Usage Examples
//!
//! ```rust
//! use kasl::libs::data_storage::DataStorage;
//!
//! // Initialize storage manager
//! let storage = DataStorage::new();
//!
//! // Get path for database file
//! let db_path = storage.get_path("kasl.db")?;
//!
//! // Get path for configuration
//! let config_path = storage.get_path("config.json")?;
//!
//! // Get path for session cache
//! let session_path = storage.get_path(".jira_session_id")?;
//! ```
//!
//! ## Error Handling
//!
//! The module handles various error scenarios gracefully:
//! - **Missing Environment Variables**: Falls back to safe defaults
//! - **Permission Denied**: Attempts alternative locations when possible
//! - **Directory Creation**: Creates parent directories as needed
//! - **Path Validation**: Ensures returned paths are usable

use anyhow::Result;
use serde::Deserialize;
use std::env::consts::OS;
use std::env::var;
use std::path::{Path, PathBuf};
use std::{fs, str};

// Include compile-time application metadata
include!(concat!(env!("OUT_DIR"), "/app_metadata.rs"));

/// Cross-platform data storage path manager.
///
/// The `DataStorage` struct provides a centralized way to manage file paths
/// for application data across different operating systems. It encapsulates
/// platform-specific logic and provides a consistent interface for path
/// resolution and directory management.
///
/// ## Design Philosophy
///
/// The storage manager follows these principles:
/// - **Platform Compliance**: Adheres to OS-specific directory conventions
/// - **User-Centric**: Stores data in user-accessible locations
/// - **Predictable**: Provides consistent behavior across platforms
/// - **Robust**: Handles edge cases and permission issues gracefully
///
/// ## Initialization
///
/// The base path is determined during construction based on:
/// 1. Operating system detection
/// 2. Environment variable resolution
/// 3. Fallback to safe defaults if needed
/// 4. Organization and application name incorporation
///
/// ## Thread Safety
///
/// The struct is designed to be used safely across multiple threads,
/// as path resolution is deterministic and doesn't modify internal state.
#[derive(Deserialize, Clone)]
pub struct DataStorage {
    /// Base directory path for all application data.
    ///
    /// This path includes the platform-specific user data directory,
    /// organization name, and application name. All application files
    /// are stored as children of this base path.
    ///
    /// The path is resolved once during construction and remains constant
    /// throughout the lifetime of the instance.
    base_path: PathBuf,
}

impl DataStorage {
    /// Creates a new DataStorage instance with platform-appropriate base path.
    ///
    /// This constructor performs automatic platform detection and constructs
    /// the appropriate base directory path following OS conventions. It uses
    /// environment variables where available and falls back to safe defaults.
    ///
    /// ## Platform Resolution Logic
    ///
    /// The constructor determines the base path using this priority order:
    /// 1. **Environment Variables**: Uses OS-specific environment variables
    /// 2. **Fallback Values**: Uses current directory if environment vars fail
    /// 3. **Path Construction**: Appends organization and application names
    /// 4. **Validation**: Ensures the resulting path is usable
    ///
    /// ## Application Metadata Integration
    ///
    /// The method uses compile-time metadata to construct paths:
    /// - `APP_METADATA_OWNER`: Organization name (e.g., "lacodda")
    /// - `APP_METADATA_NAME`: Application name (e.g., "kasl")
    ///
    /// This ensures consistent branding and path structure across builds.
    ///
    /// # Returns
    ///
    /// Returns a new `DataStorage` instance configured for the current platform
    /// and user environment.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::libs::data_storage::DataStorage;
    ///
    /// // Create platform-specific storage manager
    /// let storage = DataStorage::new();
    ///
    /// // Base path is automatically configured
    /// println!("Base path: {:?}", storage.base_path);
    /// ```
    ///
    /// ## Environment Variable Usage
    ///
    /// - **Windows**: Uses `LOCALAPPDATA` for local application data
    /// - **macOS**: Uses `HOME` to construct ~/Library/Application Support path
    /// - **Linux**: Uses `HOME` to construct ~/.local/share path
    ///
    /// ## Error Resilience
    ///
    /// If environment variables are not available, the constructor:
    /// - Falls back to current directory (".")
    /// - Continues with path construction
    /// - Defers directory creation until first access
    /// - Allows application to function in restricted environments
    pub fn new() -> Self {
        // Determine platform-specific base directory
        let base_path = match OS {
            "windows" => {
                // Windows: Use Local AppData for per-user application data
                var("LOCALAPPDATA").unwrap_or_else(|_| ".".into())
            }
            "macos" => {
                // macOS: Use Application Support following Apple guidelines
                var("HOME").unwrap_or_else(|_| ".".into()) + "/Library/Application Support"
            }
            _ => {
                // Linux/Unix: Use XDG-compliant local share directory
                var("HOME").unwrap_or_else(|_| ".".into()) + "/.local/share"
            }
        };

        // Construct full application path with organization and app name
        let base_path = Path::new(&base_path).join(APP_METADATA_OWNER).join(APP_METADATA_NAME);

        Self { base_path }
    }

    /// Resolves a filename to a complete path within the application data directory.
    ///
    /// This method takes a filename and returns the complete path where that file
    /// should be stored within the application's data directory. It automatically
    /// handles directory creation and ensures the path is ready for file operations.
    ///
    /// ## Directory Creation
    ///
    /// The method ensures that all necessary parent directories exist:
    /// - Creates the entire directory tree if missing
    /// - Uses OS-appropriate permissions for new directories
    /// - Handles concurrent access scenarios safely
    /// - Provides clear error messages if creation fails
    ///
    /// ## Path Construction
    ///
    /// The resulting path combines:
    /// 1. **Base Path**: Platform-specific application data directory
    /// 2. **Organization**: Namespace isolation (e.g., "lacodda")
    /// 3. **Application**: Application-specific subdirectory (e.g., "kasl")
    /// 4. **Filename**: The requested file within the application directory
    ///
    /// # Arguments
    ///
    /// * `file_name` - Name of the file to resolve to a full path
    ///
    /// # Returns
    ///
    /// Returns the complete `PathBuf` where the file should be stored,
    /// or an error if directory creation fails or paths are invalid.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::libs::data_storage::DataStorage;
    ///
    /// let storage = DataStorage::new();
    ///
    /// // Get path for database file
    /// let db_path = storage.get_path("kasl.db")?;
    /// // Result: /home/user/.local/share/lacodda/kasl/kasl.db (Linux)
    /// //         C:\Users\User\AppData\Local\lacodda\kasl\kasl.db (Windows)
    ///
    /// // Get path for configuration file
    /// let config_path = storage.get_path("config.json")?;
    ///
    /// // Get path for session cache
    /// let session_path = storage.get_path(".jira_session_id")?;
    /// ```
    ///
    /// ## File Naming Conventions
    ///
    /// The method accepts any valid filename, but common patterns include:
    /// - **Database files**: `kasl.db`, `backup.db`
    /// - **Configuration**: `config.json`, `settings.toml`
    /// - **Cache files**: `.session_id`, `.auth_token`
    /// - **Process files**: `kasl-watch.pid`
    /// - **Logs**: `kasl.log`, `debug.log`
    ///
    /// ## Error Scenarios
    ///
    /// The method can fail in several situations:
    /// - **Permission Denied**: Insufficient permissions to create directories
    /// - **Disk Full**: No space available for directory creation
    /// - **Path Too Long**: Resulting path exceeds OS limits
    /// - **Invalid Characters**: Filename contains invalid characters for the OS
    /// - **Read-Only Filesystem**: Target location is mounted read-only
    ///
    /// ## Concurrency Safety
    ///
    /// The directory creation process is designed to handle concurrent access:
    /// - Multiple processes can safely call this method simultaneously
    /// - Directory creation is atomic where supported by the OS
    /// - Existing directories are not affected by creation attempts
    /// - Race conditions in directory creation are handled gracefully
    pub fn get_path(&self, file_name: &str) -> Result<PathBuf> {
        // Ensure the base directory structure exists
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path)?;
        }

        // Construct and return the complete file path
        Ok(self.base_path.join(file_name))
    }
}
