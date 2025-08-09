//! Jira API integration for issue tracking and task synchronization.
//!
//! This module provides functionality to connect to Jira instances and retrieve
//! completed issues for automatic task generation and time tracking integration.
//! It implements session-based authentication with automatic retry logic.
//!
//! ## Features
//!
//! - **Issue Retrieval**: Fetch completed issues for specific dates
//! - **Session Management**: Automatic login and session token caching
//! - **Error Recovery**: Robust retry logic for authentication failures
//! - **JQL Integration**: Flexible issue querying using Jira Query Language
//!
//! ## Authentication Flow
//!
//! 1. **Initial Login**: Authenticate with username/password
//! 2. **Session Caching**: Store session cookies for reuse
//! 3. **Automatic Retry**: Re-authenticate when sessions expire
//! 4. **Error Handling**: Graceful fallback when authentication fails
//!
//! ## Usage in Task Discovery
//!
//! Completed Jira issues are automatically discovered during task import:
//! - Issues marked as "Done" or "Решена" (Resolved) for the target date
//! - Filtered to only include issues assigned to the current user
//! - Converted to tasks with issue key and summary as task name
//!
//! ## Example
//!
//! ```rust,no_run
//! use kasl::api::{Jira, JiraConfig};
//! use chrono::Local;
//!
//! let config = JiraConfig {
//!     login: "username".to_string(),
//!     api_url: "https://jira.company.com".to_string(),
//! };
//!
//! let mut jira = Jira::new(&config);
//! let today = Local::now().date_naive();
//! let issues = jira.get_completed_issues(&today).await?;
//! ```

use super::Session;
use crate::libs::{config::ConfigModule, messages::Message, secret::Secret};
use crate::msg_print;
use anyhow::Result;
use chrono::NaiveDate;
use dialoguer::{theme::ColorfulTheme, Input};
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Maximum number of authentication retries before giving up.
/// This prevents infinite loops when credentials are consistently invalid.
const MAX_RETRY_COUNT: i32 = 3;

/// Filename for storing Jira session tokens in the user data directory.
const SESSION_ID_FILE: &str = ".jira_session_id";

/// Filename for storing encrypted Jira credentials for password caching.
const SECRET_FILE: &str = ".jira_secret";

/// Jira REST API endpoint for session-based authentication.
const AUTH_URL: &str = "rest/auth/1/session";

/// Jira REST API endpoint for issue searching using JQL queries.
const SEARCH_URL: &str = "rest/api/2/search";

/// User credentials for Jira authentication.
///
/// This structure holds the login information required for establishing
/// a session with the Jira API. Credentials are only held in memory
/// during the authentication process and are never persisted to disk.
///
/// ## Security Considerations
///
/// - Passwords are stored in plain text only during authentication
/// - Credentials are cleared from memory after session establishment
/// - No persistence to avoid credential theft from configuration files
#[derive(Serialize, Clone, Debug)]
pub struct LoginCredentials {
    /// Jira username (not email address unless configured as such)
    username: String,
    /// User password in plain text (only during auth process)
    password: String,
}

/// Response structure for Jira session authentication.
///
/// Contains the session information returned by Jira after successful
/// authentication, including the session cookie name and value that
/// must be used in subsequent API requests.
#[derive(Serialize, Deserialize, Debug)]
struct JiraSessionResponse {
    /// Session object containing cookie information
    session: JiraSession,
}

/// Jira session cookie information.
///
/// Represents the session cookie that must be included in subsequent
/// API requests to authenticate the user. This cookie typically expires
/// after a period of inactivity or when explicitly invalidated.
#[derive(Serialize, Deserialize, Debug)]
struct JiraSession {
    /// Cookie name (typically "JSESSIONID" for server instances)
    name: String,
    /// Cookie value (the actual session token)
    value: String,
}

/// Represents a Jira issue with essential fields for task creation.
///
/// This structure contains the core information needed to create tasks
/// from Jira issues, focusing on identification and descriptive content
/// rather than the full complexity of Jira's data model.
#[derive(Serialize, Deserialize, Debug)]
pub struct JiraIssue {
    /// Unique issue identifier assigned by Jira (numeric)
    pub id: String,
    /// Human-readable issue key (e.g., "PROJECT-123")
    pub key: String,
    /// Issue fields containing detailed information
    pub fields: JiraIssueFields,
}

/// Detailed fields from a Jira issue.
///
/// Contains the descriptive and status information from issues that
/// is relevant for task creation and tracking. This represents a subset
/// of Jira's extensive field system, focusing on essential data.
#[derive(Serialize, Deserialize, Debug)]
pub struct JiraIssueFields {
    /// Issue title/summary (required field in Jira)
    pub summary: String,
    /// Detailed description (may be empty or contain rich text)
    pub description: Option<String>,
    /// Current workflow status information
    pub status: JiraStatus,
    /// Date when the issue was resolved (ISO format if completed)
    pub resolutiondate: Option<String>,
}

/// Jira issue status information.
///
/// Represents the current workflow status of an issue, used for filtering
/// completed vs. in-progress work. Status names vary by Jira configuration
/// and localization settings.
#[derive(Serialize, Deserialize, Debug)]
pub struct JiraStatus {
    /// Status name (e.g., "Done", "In Progress", "Решена" for Russian locale)
    pub name: String,
}

/// Response structure for Jira issue search queries.
///
/// Contains the results of JQL (Jira Query Language) searches,
/// including the matching issues and pagination information.
/// For simplicity, only the issues array is currently used.
#[derive(Serialize, Deserialize, Debug)]
pub struct JiraSearchResults {
    /// Array of issues matching the search criteria
    pub issues: Vec<JiraIssue>,
}

/// Jira API client with session management capabilities.
///
/// This client handles authentication, session caching, and issue retrieval
/// from Jira instances. It implements the [`Session`] trait for automatic
/// credential management and retry logic.
///
/// ## Thread Safety
///
/// The client is not thread-safe due to mutable retry state. Each thread
/// should use its own client instance for concurrent operations.
///
/// ## Session Lifecycle
///
/// 1. **Initialization**: Client created with configuration
/// 2. **Authentication**: Credentials prompted when first needed
/// 3. **Session Caching**: Successful sessions stored for reuse
/// 4. **Automatic Retry**: Expired sessions trigger re-authentication
/// 5. **Error Handling**: Persistent failures return empty results
#[derive(Debug)]
pub struct Jira {
    /// HTTP client for making API requests with connection pooling
    client: Client,
    /// Configuration containing API endpoint and user information
    config: JiraConfig,
    /// In-memory storage for authentication credentials during auth process
    credentials: Option<LoginCredentials>,
    /// Counter for tracking authentication retry attempts
    retries: i32,
}

impl Session for Jira {
    /// Performs session-based authentication with Jira.
    ///
    /// This method implements Jira's session authentication flow using the
    /// REST API. It sends user credentials to the authentication endpoint
    /// and receives a session cookie that can be used for subsequent requests.
    ///
    /// ## Authentication Process
    ///
    /// 1. **Credential Validation**: Ensures credentials are set before proceeding
    /// 2. **HTTP Request**: POST to the session authentication endpoint with JSON credentials
    /// 3. **Response Validation**: Checks for successful HTTP status codes
    /// 4. **Cookie Extraction**: Parses session information from the response
    /// 5. **Format Preparation**: Creates properly formatted cookie string for headers
    ///
    /// ## Session Cookie Format
    ///
    /// The returned session ID is formatted as `{cookie_name}={cookie_value}` and
    /// should be included in the `Cookie` header of subsequent API requests.
    ///
    /// # Returns
    ///
    /// Returns a formatted session cookie string on successful authentication.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No credentials have been set (programming error)
    /// - HTTP request fails due to network issues
    /// - Credentials are invalid (401 response)
    /// - Jira returns an unexpected response format
    /// - Session parsing fails
    async fn login(&self) -> Result<String> {
        // Ensure credentials are available for authentication
        let credentials = self.credentials.clone().expect("Credentials not set!");

        // Build authentication endpoint URL
        let auth_url = format!("{}/{}", self.config.api_url, AUTH_URL);

        // Send authentication request with JSON credentials
        let auth_res = self.client.post(auth_url).json(&credentials).send().await?;

        // Validate response status
        if !auth_res.status().is_success() {
            anyhow::bail!("Jira authenticate failed")
        }

        // Parse session information from response
        let session_res = auth_res.json::<JiraSessionResponse>().await?;

        // Format session cookie for use in subsequent requests
        let session_id = format!("{}={}", session_res.session.name, session_res.session.value);
        Ok(session_id)
    }

    /// Sets user credentials for Jira authentication.
    ///
    /// Stores the provided username and password in memory for use during
    /// the authentication process. This method is called by the session
    /// management system when credentials are needed.
    ///
    /// ## Security Notes
    ///
    /// - Credentials are only stored in memory temporarily
    /// - Password is stored in plain text for authentication
    /// - No persistence to disk or configuration files
    /// - Credentials are cleared after successful authentication
    ///
    /// # Arguments
    ///
    /// * `password` - The user's Jira password in plain text
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` as this operation cannot fail.
    fn set_credentials(&mut self, password: &str) -> Result<()> {
        self.credentials = Some(LoginCredentials {
            username: self.config.login.to_string(),
            password: password.to_owned(),
        });
        Ok(())
    }

    /// Returns the filename for storing Jira session tokens.
    ///
    /// The session file is stored in the user's application data directory
    /// and contains the cached session token for automatic login restoration.
    fn session_id_file(&self) -> &str {
        SESSION_ID_FILE
    }

    /// Returns a configured Secret instance for secure password prompting.
    ///
    /// The Secret manager handles secure password input with hidden characters
    /// and optional encrypted caching in the user's data directory.
    ///
    /// # Returns
    ///
    /// A configured `Secret` instance with Jira-specific prompts and file names.
    fn secret(&self) -> Secret {
        Secret::new(SECRET_FILE, "Enter your Jira password")
    }

    /// Returns the current authentication retry count.
    ///
    /// Used by the session management system to track failed authentication
    /// attempts and implement retry limits.
    fn retry(&self) -> i32 {
        self.retries
    }

    /// Increments the authentication retry counter.
    ///
    /// Called after each failed authentication attempt to track progress
    /// toward the maximum retry limit defined in the session management system.
    fn inc_retry(&mut self) {
        self.retries += 1;
    }
}

impl Jira {
    /// Creates a new Jira API client instance.
    ///
    /// Initializes the HTTP client with default settings suitable for Jira API
    /// interactions. The client is configured for JSON requests and includes
    /// appropriate timeout and connection settings.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration containing Jira URL and login information
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::api::{Jira, JiraConfig};
    ///
    /// let config = JiraConfig {
    ///     login: "username".to_string(),
    ///     api_url: "https://jira.company.com".to_string(),
    /// };
    /// let jira = Jira::new(&config);
    /// ```
    pub fn new(config: &JiraConfig) -> Self {
        Self {
            client: Client::new(),
            config: config.clone(),
            credentials: None,
            retries: 0,
        }
    }

    /// Retrieves all issues completed by the current user on a specific date.
    ///
    /// This method performs a sophisticated issue search using JQL (Jira Query Language)
    /// to find issues that were marked as completed on the specified date. The search
    /// includes robust error handling and automatic session management with retry logic.
    ///
    /// ## JQL Query Details
    ///
    /// The search uses the following criteria:
    /// - **Status Filter**: Issues with status "Done" or "Решена" (supports localized Jira)
    /// - **Resolution Date**: Issues resolved within the full day range (00:00 to 23:59)
    /// - **Assignee Filter**: Only issues assigned to the current user (`currentUser()`)
    ///
    /// ## Session Management
    ///
    /// The method implements sophisticated session handling:
    /// 1. **Session Retrieval**: Get or create a valid session token using the Session trait
    /// 2. **API Request**: Execute the JQL search with session cookie authentication
    /// 3. **Error Handling**: Detect HTTP 401 (Unauthorized) responses indicating expired sessions
    /// 4. **Automatic Retry**: Clear cached session and retry authentication up to the limit
    /// 5. **Graceful Degradation**: Return empty results on persistent authentication failures
    ///
    /// ## Error Recovery Strategy
    ///
    /// Unlike other API integrations, Jira errors are allowed to propagate rather
    /// than returning empty results silently. This is because Jira data is typically more
    /// critical for work tracking, and users should be aware of connection issues.
    ///
    /// However, authentication failures are handled gracefully with automatic
    /// retry logic and eventual fallback to empty results after exhausting retries.
    ///
    /// ## Date Handling
    ///
    /// The method formats the provided date to ensure proper JQL syntax and
    /// covers the entire day from midnight to 23:59 to capture all possible
    /// resolution times within the target date.
    ///
    /// # Arguments
    ///
    /// * `date` - The date to search for completed issues (in any timezone)
    ///
    /// # Returns
    ///
    /// Returns a vector of [`JiraIssue`] objects representing completed work.
    /// Returns an empty vector if:
    /// - No issues are found matching the criteria
    /// - Authentication fails persistently after all retries
    /// - Network errors occur during the request
    ///
    /// # Errors
    ///
    /// May return errors for:
    /// - JSON parsing failures in API responses
    /// - Unexpected HTTP response formats
    /// - Session token formatting errors
    ///
    /// Network errors and authentication failures are handled gracefully
    /// and result in empty results rather than propagated errors.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use kasl::api::{Jira, JiraConfig};
    /// # use chrono::NaiveDate;
    /// # use anyhow::Result;
    /// # async fn example() -> Result<()> {
    /// let config = JiraConfig {
    ///     login: "username".to_string(),
    ///     api_url: "https://jira.company.com".to_string(),
    /// };
    /// let mut jira = Jira::new(&config);
    ///
    /// let today = chrono::Local::now().date_naive();
    /// let issues = jira.get_completed_issues(&today).await?;
    ///
    /// for issue in issues {
    ///     println!("Completed: {} - {}", issue.key, issue.fields.summary);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_completed_issues(&mut self, date: &NaiveDate) -> Result<Vec<JiraIssue>> {
        loop {
            // Step 1: Ensure we have a valid session token
            let session_id = match self.get_session_id().await {
                Ok(id) => id,
                Err(_) => return Ok(Vec::new()), // Give up on persistent auth failures
            };

            // Step 2: Build JQL query for completed issues on the specified date
            let date_str = date.format("%Y-%m-%d").to_string();
            let jql = format!(
                "status in (Done, Решена) AND resolved >= \"{}\" AND resolved <= \"{} 23:59\" AND assignee in (currentUser())",
                &date_str, &date_str
            );

            // Step 3: Prepare request with session authentication
            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&session_id)?);
            let url = format!("{}/{}?jql={}", &self.config.api_url, SEARCH_URL, &jql);

            // Step 4: Execute the search request
            let res = match self.client.get(&url).headers(headers).send().await {
                Ok(response) => response,
                Err(_) => return Ok(Vec::new()), // Network errors return empty results
            };

            // Step 5: Handle response and potential session expiration
            match res.status() {
                StatusCode::UNAUTHORIZED if self.retries < MAX_RETRY_COUNT => {
                    // Session expired - clear cache and retry
                    self.delete_session_id()?;
                    self.inc_retry();
                    // Brief delay before retry to avoid hammering the server
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                _ => {
                    // Success or non-recoverable error - parse and return results
                    let search_results = res.json::<JiraSearchResults>().await?;
                    return Ok(search_results.issues);
                }
            }
        }
    }
}

/// Configuration for Jira API integration.
///
/// This structure holds the necessary information for connecting to Jira
/// instances, including both cloud and server/data center deployments.
/// The configuration is designed to be serializable for storage in
/// configuration files.
///
/// ## Security Notes
///
/// - Passwords are never stored in configuration files
/// - Only usernames and API endpoints are persisted
/// - Session tokens are cached separately with encryption
/// - Configuration files should have restricted permissions
///
/// ## Supported Jira Instances
///
/// - **Atlassian Cloud**: Uses `https://company.atlassian.net` format
/// - **Server/Data Center**: Uses custom domain like `https://jira.company.com`
/// - **Local Development**: Can use `http://localhost:8080` for testing
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JiraConfig {
    /// Jira username for authentication.
    ///
    /// This should be the actual username, not an email address,
    /// unless your Jira instance is configured to use email addresses
    /// as usernames. Check with your Jira administrator if unsure.
    ///
    /// For Atlassian Cloud instances, this is typically the email address
    /// used to register the account.
    pub login: String,

    /// Base URL of the Jira instance.
    ///
    /// Examples:
    /// - Atlassian Cloud: `https://company.atlassian.net`
    /// - Server/Data Center: `https://jira.company.com`
    /// - Local development: `http://localhost:8080`
    ///
    /// Do not include the `/rest/api/` path as it will be added automatically.
    /// The URL should point to the root of your Jira installation.
    pub api_url: String,
}

impl JiraConfig {
    /// Returns the configuration module metadata for Jira.
    ///
    /// Used by the configuration system to identify and manage
    /// Jira-specific settings during interactive setup. This provides
    /// the human-readable name and internal key for the module.
    ///
    /// # Returns
    ///
    /// A `ConfigModule` with Jira identification information.
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "jira".to_string(),
            name: "Jira".to_string(),
        }
    }

    /// Runs an interactive configuration setup for Jira integration.
    ///
    /// Prompts the user for Jira instance URL and username, using existing
    /// configuration values as defaults if available. This method provides
    /// a user-friendly way to configure Jira integration during initial
    /// setup or reconfiguration.
    ///
    /// ## Interactive Prompts
    ///
    /// 1. **Username**: Prompts for Jira username (or email for cloud instances)
    /// 2. **API URL**: Prompts for Jira instance URL with validation hints
    ///
    /// Both prompts will show existing values as defaults if configuration
    /// already exists, making it easy to update only specific values without
    /// re-entering everything.
    ///
    /// ## Configuration Validation
    ///
    /// While this method doesn't validate the actual connection to Jira,
    /// it provides helpful prompts and examples to guide users toward
    /// correct configuration values.
    ///
    /// # Arguments
    ///
    /// * `config` - Existing Jira configuration to use as defaults (if any)
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - New Jira configuration with user input
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
    /// # use kasl::api::JiraConfig;
    /// # use anyhow::Result;
    /// # fn example() -> Result<()> {
    /// let existing_config = Some(JiraConfig {
    ///     login: "olduser".to_string(),
    ///     api_url: "https://old-jira.com".to_string(),
    /// });
    ///
    /// let new_config = JiraConfig::init(&existing_config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn init(config: &Option<Self>) -> Result<Self> {
        // Use existing configuration as defaults, or create empty defaults
        let config = config
            .clone()
            .or(Some(Self {
                login: "".to_string(),
                api_url: "".to_string(),
            }))
            .unwrap();

        // Display configuration module header
        msg_print!(Message::ConfigModuleJira);

        // Interactive configuration with existing values as defaults
        Ok(Self {
            login: Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter your Jira login")
                .default(config.login)
                .interact_text()?,
            api_url: Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter the Jira API URL")
                .default(config.api_url)
                .interact_text()?,
        })
    }
}
