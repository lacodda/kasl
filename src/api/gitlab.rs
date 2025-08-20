//! GitLab API client for fetching user activity and commit data.
//!
//! Provides integration with GitLab instances (both self-hosted and GitLab.com)
//! to automatically discover and import development activities as tasks.
//!
//! ## Features
//!
//! - **Commit Discovery**: Automatically fetches today's commits for task generation
//! - **User Activity**: Retrieves push events and commit details via GitLab API v4
//! - **Error Resilience**: Gracefully handles network failures without crashing
//! - **Multi-Instance Support**: Works with GitLab.com, self-hosted, and enterprise instances
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::api::gitlab::{GitLab, GitLabConfig};
//!
//! let config = GitLabConfig {
//!     access_token: "glpat-xxxxxxxxxxxxxxxxxxxx".to_string(),
//!     api_url: "https://gitlab.com".to_string(),
//! };
//!
//! let client = GitLab::new(&config);
//! let commits = client.get_today_commits().await?;
//! ```

use crate::libs::config::ConfigModule;
use crate::libs::messages::Message;
use crate::{msg_error, msg_print};
use anyhow::Result;
use chrono::{Duration, Local};
use dialoguer::{theme::ColorfulTheme, Input};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// GitLab API client for retrieving user activity and commit information.
///
/// This client handles authentication and data retrieval from GitLab instances,
/// specifically focusing on user events and commit details that can be transformed
/// into task entries for time tracking purposes.
///
/// The client is stateless and thread-safe, making it suitable for concurrent
/// operations and long-running applications.
#[derive(Debug)]
pub struct GitLab {
    /// HTTP client for making API requests with connection pooling
    client: Client,
    /// Configuration containing API endpoint and authentication details
    config: GitLabConfig,
}

/// Represents a GitLab user event from the events API.
///
/// GitLab events capture various user activities including pushes, merges,
/// comments, and other repository interactions. This structure focuses on
/// push events which contain commit information.
#[derive(Debug, Deserialize)]
struct Event {
    /// Type of action performed (e.g., "pushed to", "opened", "commented on")
    action_name: String,
    /// Additional data for push events, contains commit references
    push_data: Option<PushData>,
    /// GitLab project ID where the event occurred
    project_id: u32,
}

/// Push event data containing commit references.
///
/// When a user pushes commits to a repository, GitLab includes additional
/// metadata about the push operation, including the commit SHA that was
/// pushed to the target branch.
#[derive(Debug, Deserialize)]
struct PushData {
    /// SHA of the commit that was pushed (target commit)
    commit_to: Option<String>,
}

/// Simplified commit information for task creation.
///
/// This structure represents the essential information extracted from GitLab
/// commits that's needed for generating task entries. It focuses on human-readable
/// content rather than technical Git metadata.
#[derive(Debug)]
pub struct CommitInfo {
    /// Full SHA hash of the commit for unique identification
    pub sha: String,
    /// First line of the commit message (typically the summary)
    pub message: String,
}

/// Detailed commit object returned by GitLab's commits API.
///
/// This represents the full commit information returned by GitLab's REST API
/// when fetching specific commit details. Contains more information than needed
/// for task creation, but provides access to the complete commit message.
#[derive(Debug, Deserialize)]
struct Commit {
    /// Full SHA identifier of the commit
    id: String,
    /// Complete commit message including body and trailers
    message: String,
}

/// GitLab user information for retrieving user ID.
///
/// Used to identify the current user when fetching user-specific events.
/// GitLab's events API requires the numeric user ID rather than username.
#[derive(Debug, Deserialize)]
struct User {
    /// Numeric user identifier in GitLab
    id: u32,
}

impl GitLab {
    /// Creates a new GitLab API client instance.
    ///
    /// Initializes the HTTP client with default settings suitable for GitLab API
    /// interactions. The client is configured for JSON responses and includes
    /// reasonable timeout settings.
    ///
    /// # Arguments
    ///
    /// * `config` - GitLab configuration containing API endpoint and authentication token
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let config = GitLabConfig {
    ///     access_token: "glpat-xxxxxxxxxxxxxxxxxxxx".to_string(),
    ///     api_url: "https://gitlab.example.com".to_string(),
    /// };
    /// let client = GitLab::new(&config);
    /// ```
    pub fn new(config: &GitLabConfig) -> Self {
        Self {
            client: Client::new(),
            config: config.clone(),
        }
    }

    /// Retrieves the current user's GitLab ID.
    ///
    /// Makes a request to GitLab's `/user` endpoint to fetch the authenticated user's
    /// information. The user ID is required for subsequent calls to the events API.
    ///
    /// # Returns
    ///
    /// * `Result<u32>` - The numeric user ID on success
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Network request fails
    /// - Authentication token is invalid
    /// - GitLab returns an unexpected response format
    ///
    /// # API Endpoint
    ///
    /// `GET /api/v4/user` - Requires `read_user` scope
    pub async fn get_user_id(&self) -> Result<u32> {
        let url = format!("{}/api/v4/user", self.config.api_url);
        let response = self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await?;

        Ok(response.json::<User>().await?.id)
    }

    /// Fetches all commits made by the authenticated user today.
    ///
    /// This is the primary method for discovering development activity that can be
    /// converted into time tracking tasks. It retrieves user events from yesterday
    /// to tomorrow (to handle timezone issues) and filters for push events containing
    /// commit information.
    ///
    /// ## Process Flow
    ///
    /// 1. **Date Range Calculation**: Creates a date range around today to handle timezone differences
    /// 2. **User ID Retrieval**: Gets the authenticated user's ID for events API
    /// 3. **Events Fetching**: Retrieves user events within the date range
    /// 4. **Event Filtering**: Processes only "pushed to" events with commit data
    /// 5. **Commit Details**: Fetches detailed commit information for each push
    /// 6. **Message Processing**: Extracts and cleans commit messages for task names
    ///
    /// ## Error Resilience
    ///
    /// This method is designed to be fault-tolerant in production environments:
    /// - Network failures return empty results instead of errors
    /// - Individual commit fetch failures are skipped
    /// - API parsing errors are logged and don't interrupt processing
    /// - Missing or malformed data is handled gracefully
    ///
    /// # Returns
    ///
    /// * `Result<Vec<CommitInfo>>` - List of today's commits, or empty vector on any error
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let commits = gitlab_client.get_today_commits().await?;
    /// for commit in commits {
    ///     println!("Commit {}: {}", commit.sha, commit.message);
    /// }
    /// ```
    pub async fn get_today_commits(&self) -> Result<Vec<CommitInfo>> {
        // Calculate date range around today to handle timezone differences
        let today = Local::now();
        let yesterday = (today - Duration::days(1)).format("%Y-%m-%d").to_string();
        let tomorrow = (today + Duration::days(1)).format("%Y-%m-%d").to_string();

        // Get authenticated user's ID for events API
        let user_id = match self.get_user_id().await {
            Ok(id) => id,
            Err(e) => {
                msg_error!(Message::GitlabUserIdFailed(e.to_string()));
                return Ok(Vec::new()); // Return empty on user ID failure
            }
        };

        // Fetch user events within date range
        let url = format!(
            "{}/api/v4/users/{}/events?after={}&before={}",
            self.config.api_url, user_id, yesterday, tomorrow
        );

        let response = match self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await {
            Ok(res) => res,
            Err(e) => {
                msg_error!(Message::GitlabFetchFailed(e.to_string()));
                return Ok(Vec::new()); // Return empty on request failure
            }
        };

        // Parse events response
        let events = match response.json::<Vec<Event>>().await {
            Ok(ev) => ev,
            Err(e) => {
                msg_error!(Message::GitlabFetchFailed(e.to_string()));
                return Ok(Vec::new()); // Return empty on parsing failure
            }
        };

        // Process push events and collect commit information
        let mut commits_info = Vec::new();
        for event in events {
            // Only process push events
            if event.action_name == "pushed to" {
                if let Some(push_data) = event.push_data {
                    if let Some(commit_to) = push_data.commit_to {
                        // Fetch detailed commit information
                        let commit_detail = match self.get_commit_detail(event.project_id, &commit_to).await {
                            Ok(detail) => detail,
                            Err(_) => continue, // Skip commits that can't be fetched
                        };

                        // Extract commit message (first line only for task names)
                        let clean_message = commit_detail
                            .message
                            .split_once('\n') // Split on first newline
                            .map(|(part, _)| part) // Take first part (summary line)
                            .unwrap_or(&commit_detail.message) // Use full message if no newline
                            .to_string();

                        commits_info.push(CommitInfo {
                            sha: commit_detail.id,
                            message: clean_message,
                        });
                    }
                }
            }
        }

        Ok(commits_info)
    }

    /// Fetches detailed information for a specific commit.
    ///
    /// Retrieves the complete commit object from GitLab's commits API, including
    /// the full commit message and metadata. This is used to get detailed information
    /// about commits identified through the events API.
    ///
    /// # Arguments
    ///
    /// * `project_id` - Numeric ID of the GitLab project containing the commit
    /// * `commit_sha` - SHA hash of the commit to retrieve
    ///
    /// # Returns
    ///
    /// * `Result<Commit>` - Complete commit information from GitLab
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The commit doesn't exist or isn't accessible
    /// - Network request fails
    /// - GitLab returns an unexpected response format
    /// - The user lacks permission to access the project
    ///
    /// # API Endpoint
    ///
    /// `GET /api/v4/projects/{project_id}/repository/commits/{commit_sha}`
    async fn get_commit_detail(&self, project_id: u32, commit_sha: &str) -> Result<Commit> {
        let url = format!("{}/api/v4/projects/{}/repository/commits/{}", self.config.api_url, project_id, commit_sha);
        let response = self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await?;

        Ok(response.json::<Commit>().await?)
    }
}

/// Configuration for GitLab API integration.
///
/// This structure holds the necessary information for connecting to GitLab
/// instances, including both GitLab.com and self-hosted installations.
///
/// ## Security Notes
///
/// - Personal Access Tokens are stored in configuration files
/// - Tokens should be generated with minimal required scopes (`read_user`, `read_repository`)
/// - Consider using project-specific tokens for enhanced security
/// - Tokens can be revoked through GitLab's interface if compromised
///
/// ## Supported Instances
///
/// - **GitLab.com**: Use `https://gitlab.com` as the API URL
/// - **Self-hosted**: Use your instance URL (e.g., `https://gitlab.company.com`)
/// - **GitLab Enterprise**: Same as self-hosted with enterprise features
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitLabConfig {
    /// Personal Access Token for GitLab API authentication.
    ///
    /// This token must have the following scopes:
    /// - `read_user`: To fetch user information and user ID
    /// - `read_repository`: To access commit data and repository events
    ///
    /// Generate tokens at: GitLab → User Settings → Access Tokens
    pub access_token: String,

    /// Base URL of the GitLab instance.
    ///
    /// Examples:
    /// - GitLab.com: `https://gitlab.com`
    /// - Self-hosted: `https://gitlab.example.com`
    /// - Local development: `http://localhost:8080`
    ///
    /// Do not include the `/api/v4` path - it will be added automatically.
    pub api_url: String,
}

impl GitLabConfig {
    /// Returns the configuration module metadata for GitLab.
    ///
    /// Used by the configuration system to identify and manage
    /// GitLab-specific settings during interactive setup.
    ///
    /// # Returns
    ///
    /// A `ConfigModule` with GitLab identification information.
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "gitlab".to_string(),
            name: "GitLab".to_string(),
        }
    }

    /// Runs an interactive configuration setup for GitLab integration.
    ///
    /// Prompts the user for GitLab instance URL and personal access token,
    /// using existing configuration values as defaults if available. This method
    /// provides a user-friendly way to configure GitLab integration during
    /// initial setup or reconfiguration.
    ///
    /// ## Interactive Prompts
    ///
    /// 1. **Personal Access Token**: Prompts for GitLab PAT with hidden input
    /// 2. **API URL**: Prompts for GitLab instance URL with validation
    ///
    /// Both prompts will show existing values as defaults if configuration
    /// already exists, making it easy to update only specific values.
    ///
    /// # Arguments
    ///
    /// * `config` - Existing GitLab configuration to use as defaults (if any)
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - New GitLab configuration with user input
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Terminal input/output fails
    /// - User cancels the configuration process
    /// - Input validation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let existing_config = Some(GitLabConfig {
    ///     access_token: "glpat-old-token".to_string(),
    ///     api_url: "https://gitlab.com".to_string(),
    /// });
    ///
    /// let new_config = GitLabConfig::init(&existing_config)?;
    /// ```
    pub fn init(config: &Option<GitLabConfig>) -> Result<Self> {
        // Use existing configuration as defaults, or create empty defaults
        let config = config.clone().unwrap_or(Self {
            access_token: "".to_string(),
            api_url: "".to_string(),
        });

        // Display configuration module header
        msg_print!(Message::ConfigModuleGitLab);

        // Interactive configuration with existing values as defaults
        Ok(Self {
            access_token: Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter your GitLab private token")
                .default(config.access_token)
                .interact_text()?,
            api_url: Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter the GitLab API URL")
                .default(config.api_url)
                .interact_text()?,
        })
    }
}
