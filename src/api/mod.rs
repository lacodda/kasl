//! External API clients (GitLab, Jira, SiServer) and the shared
//! session-management pattern they implement.
//!
//! ```text
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

pub mod gitlab;
pub mod jira;
pub mod si;

pub use gitlab::GitLabConfig;
pub use jira::JiraConfig;
pub use si::SiConfig;

/// Authentication attempts before giving up on a password.
const MAX_RETRY_COUNT: i32 = 3;

/// Session lifecycle shared by every API client: cache the session id on
/// disk, prompt for the password only when needed, retry a bounded number
/// of times.
///
/// Implementors supply the provider-specific pieces (`login`,
/// `set_credentials`, file names); the provided methods do the rest.
#[allow(async_fn_in_trait)]
pub trait Session {
    /// Authenticates with the stored credentials, returning a session id.
    async fn login(&self) -> Result<String>;

    /// Stores the password (encoded as the provider requires) for `login`.
    fn set_credentials(&mut self, password: &str) -> Result<()>;

    /// Per-provider session cache filename.
    fn session_id_file(&self) -> &str;

    /// Per-provider secret manager (prompt text, cache file).
    fn secret(&self) -> Secret;

    /// Current failed-attempt count.
    fn retry(&self) -> i32;

    /// Bumps the failed-attempt count.
    fn inc_retry(&mut self);

    /// Clears the failed-attempt count after a successful login.
    fn reset_retry(&mut self);

    /// Returns a session id: cached if available, otherwise via login with
    /// up to [`MAX_RETRY_COUNT`] password attempts (a retry always
    /// re-prompts rather than reusing a password that just failed).
    async fn get_session_id(&mut self) -> Result<String> {
        let session_id_file_path = DataStorage::new().get_path(self.session_id_file())?;
        let session_id_file_path_str = session_id_file_path.to_str().unwrap();

        if let Ok(session_id) = Self::read_session_id(session_id_file_path_str) {
            Ok(session_id)
        } else {
            loop {
                let password: String = match self.retry() > 0 {
                    true => self.secret().prompt()?,         // Force new prompt on retry
                    false => self.secret().get_or_prompt()?, // Use cache if available
                };

                self.set_credentials(&password)?;

                let session_id = self.login().await;
                match session_id {
                    Ok(session_id) => {
                        let _ = Self::write_session_id(session_id_file_path_str, &session_id);
                        self.reset_retry();
                        return Ok(session_id);
                    }
                    Err(_) => {
                        if self.retry() < MAX_RETRY_COUNT {
                            self.inc_retry();
                            continue;
                        }
                        break Err(msg_error_anyhow!(Message::WrongPassword(MAX_RETRY_COUNT)));
                    }
                }
            }
        }
    }

    /// Reads the cached session id.
    fn read_session_id(file_name: &str) -> Result<String> {
        Ok(fs::read_to_string(file_name)?)
    }

    /// Writes the session id to the cache file.
    fn write_session_id(file_name: &str, session_id: &str) -> Result<()> {
        let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).open(file_name)?;
        file.write_all(session_id.as_bytes())?;
        Ok(())
    }

    /// Drops the cached session, forcing a fresh login next time.
    fn delete_session_id(&self) -> Result<()> {
        let session_id_file_path = DataStorage::new().get_path(self.session_id_file())?;
        fs::remove_file(session_id_file_path)?;
        Ok(())
    }
}
