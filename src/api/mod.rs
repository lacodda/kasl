//! API client modules for external service integrations.
//!
//! Provides a unified interface for interacting with various external APIs
//! that kasl integrates with. Includes clients for GitLab, Jira, and internal
//! SiServer systems, all implementing a common session management pattern.
//!
//! ## Features
//!
//! - **GitLab**: Fetches user activity and commit data for task creation
//! - **Jira**: Retrieves assigned issues and project information
//! - **SiServer**: Internal reporting API for time tracking submissions
//! - **Session Management**: Automatic caching, encrypted storage, retry logic
//! - **Security**: Encrypted tokens, secure prompting, session invalidation
//!
//! ## Usage
//!
//! ```rust
//! use kasl::api::{GitLabConfig, JiraConfig, SiConfig};
//!
//! let jira_module = JiraConfig::module();
//! let jira_config = JiraConfig::init(&existing_config)?;
//! ```

use crate::libs::messages::Message;
use crate::libs::{data_storage::DataStorage, secret::Secret};
use crate::msg_error_anyhow;
use anyhow::Result;
use std::fs;
use std::io::Write;

// API client modules
pub mod gitlab;
pub mod jira;
pub mod si;

// Re-export configuration structs for easier access from other modules
pub use gitlab::GitLabConfig;
pub use jira::JiraConfig;
pub use si::SiConfig;

/// Maximum number of authentication retry attempts before giving up.
///
/// This prevents infinite loops when credentials are consistently invalid
/// and provides a reasonable number of attempts for user input errors.
const MAX_RETRY_COUNT: i32 = 3;

/// Common session management trait for all API clients.
///
/// Provides a standardized interface for handling authentication, session caching,
/// and credential management across different API providers.
#[allow(async_fn_in_trait)]
pub trait Session {
    /// Performs authentication and returns a session identifier.
    ///
    /// This method handles the actual API authentication process using stored
    /// credentials. The returned session ID can be used for subsequent API calls.
    ///
    /// # Returns
    ///
    /// * `Result<String>` - Session identifier on success, error on failure
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Network connection fails
    /// - Credentials are invalid
    /// - API returns an unexpected response format
    async fn login(&self) -> Result<String>;

    /// Sets user credentials for authentication.
    ///
    /// Stores the provided password in memory for use during authentication.
    /// The password may be encoded or hashed depending on the API requirements.
    ///
    /// # Arguments
    ///
    /// * `password` - User password in plain text
    ///
    /// # Errors
    ///
    /// Returns an error if password encoding/validation fails.
    fn set_credentials(&mut self, password: &str) -> Result<()>;

    /// Returns the filename used for session storage.
    ///
    /// Each API client uses a unique session file to avoid conflicts.
    /// Files are stored in the application's data directory with restricted permissions.
    fn session_id_file(&self) -> &str;

    /// Returns the secret manager for this API client.
    ///
    /// Provides access to encrypted credential storage and interactive prompting
    /// specific to this API provider.
    fn secret(&self) -> Secret;

    /// Returns current retry attempt count.
    ///
    /// Used to track authentication failures and implement retry limits.
    fn retry(&self) -> i32;

    /// Increments the retry counter.
    ///
    /// Called after each failed authentication attempt to track progress
    /// toward the maximum retry limit.
    fn inc_retry(&mut self);

    /// Retrieves or establishes a valid session ID.
    ///
    /// This is the main entry point for session management. It handles the complete
    /// session lifecycle including cache restoration, authentication, and retry logic.
    ///
    /// ## Process Flow
    ///
    /// 1. **Cache Check**: Attempt to restore session from encrypted storage
    /// 2. **Authentication Loop**: If no cache, prompt for credentials and authenticate
    /// 3. **Retry Logic**: Handle failures with limited retry attempts
    /// 4. **Session Storage**: Cache successful sessions for future use
    ///
    /// # Returns
    ///
    /// * `Result<String>` - Valid session ID ready for API calls
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Maximum retry attempts exceeded
    /// - Storage operations fail
    /// - Network or API errors prevent authentication
    async fn get_session_id(&mut self) -> Result<String> {
        // Attempt to restore session from encrypted cache
        let session_id_file_path = DataStorage::new().get_path(&self.session_id_file())?;
        let session_id_file_path_str = session_id_file_path.to_str().unwrap();

        if let Ok(session_id) = Self::read_session_id(&session_id_file_path_str) {
            return Ok(session_id);
        } else {
            // No valid cached session - begin authentication process
            loop {
                // Get password from cache or interactive prompt
                let password: String = match self.retry() > 0 {
                    true => self.secret().prompt()?,         // Force new prompt on retry
                    false => self.secret().get_or_prompt()?, // Use cache if available
                };

                // Set credentials for authentication
                self.set_credentials(&password)?;

                // Attempt to authenticate with the API
                let session_id = self.login().await;
                match session_id {
                    Ok(session_id) => {
                        // Success - cache the session and return
                        let _ = Self::write_session_id(&session_id_file_path_str, &session_id);
                        return Ok(session_id);
                    }
                    Err(_) => {
                        // Authentication failed - check retry limit
                        if self.retry() < MAX_RETRY_COUNT {
                            self.inc_retry();
                            continue; // Try again with new credentials
                        }
                        // Maximum retries exceeded
                        break Err(msg_error_anyhow!(Message::WrongPassword(MAX_RETRY_COUNT)));
                    }
                }
            }
        }
    }

    /// Reads a session ID from the specified file.
    ///
    /// Attempts to load a cached session identifier from disk storage.
    /// The session may be encrypted depending on the implementation.
    ///
    /// # Arguments
    ///
    /// * `file_name` - Path to the session storage file
    ///
    /// # Returns
    ///
    /// * `Result<String>` - Session ID if file exists and is readable
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist, is unreadable, or contains
    /// invalid session data.
    fn read_session_id(file_name: &str) -> Result<String> {
        Ok(fs::read_to_string(file_name)?)
    }

    /// Writes a session ID to the specified file.
    ///
    /// Stores the session identifier for future use, potentially with encryption.
    /// The file is created with restricted permissions for security.
    ///
    /// # Arguments
    ///
    /// * `file_name` - Path where session should be stored
    /// * `session_id` - Session identifier to save
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Success indicator
    ///
    /// # Errors
    ///
    /// Returns an error if file creation or writing fails.
    fn write_session_id(file_name: &str, session_id: &str) -> Result<()> {
        let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).open(file_name)?;
        file.write_all(session_id.as_bytes())?;
        Ok(())
    }

    /// Deletes the cached session file.
    ///
    /// Removes the session cache when authentication fails or sessions expire.
    /// This forces fresh authentication on the next session request.
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Success indicator
    ///
    /// # Errors
    ///
    /// Returns an error if file deletion fails. Missing files are not considered errors.
    fn delete_session_id(&self) -> Result<()> {
        let session_id_file_path = DataStorage::new().get_path(&self.session_id_file())?;
        fs::remove_file(session_id_file_path)?;
        Ok(())
    }
}
