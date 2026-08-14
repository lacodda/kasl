//! Jira client: completed issues for task discovery, assigned open
//! issues for the inbox poller.
//!
//! ```rust,no_run
//! # use kasl::api::jira::{Jira, JiraConfig};
//! # use chrono::Local;
//! # async fn f() -> anyhow::Result<()> {
//! let config = JiraConfig {
//!     login: "username".to_string(),
//!     api_url: "https://jira.company.com".to_string(),
//!     completed_statuses: Vec::new(),
//! };
//!
//! let mut jira = Jira::new(&config);
//! let today = Local::now().date_naive();
//! let issues = jira.get_completed_issues(&today).await?;
//! # Ok(())
//! # }
//! ```

use super::Session;
use crate::libs::{config::ConfigModule, messages::Message, secret::Secret};
use crate::msg_print;
use anyhow::Result;
use chrono::NaiveDate;
use dialoguer::{Input, theme::ColorfulTheme};
use reqwest::{
    Client, StatusCode,
    header::{COOKIE, HeaderMap, HeaderValue},
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

const MAX_RETRY_COUNT: i32 = 3;

const SEARCH_PAGE_SIZE: u32 = 100;

const SESSION_ID_FILE: &str = ".jira_session_id";

const SECRET_FILE: &str = ".jira_secret";

const AUTH_URL: &str = "rest/auth/1/session";

const SEARCH_URL: &str = "rest/api/2/search";

/// Login pair, held in memory only while authenticating.
#[derive(Serialize, Clone, Debug)]
pub struct LoginCredentials {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct JiraSessionResponse {
    session: JiraSession,
}

/// Session cookie (typically `JSESSIONID`) sent on every API request.
#[derive(Serialize, Deserialize, Debug)]
struct JiraSession {
    name: String,
    value: String,
}

/// The slice of a Jira issue kasl works with.
#[derive(Serialize, Deserialize, Debug)]
pub struct JiraIssue {
    /// Numeric issue id assigned by Jira.
    pub id: String,
    /// Human-readable key, e.g. "PROJECT-123".
    pub key: String,
    pub fields: JiraIssueFields,
}

/// The fields kasl reads; custom fields land in `extra` via flatten.
#[derive(Serialize, Deserialize, Debug)]
pub struct JiraIssueFields {
    /// Issue title/summary (required field in Jira)
    pub summary: String,
    /// Detailed description (may be empty or contain rich text)
    #[serde(default)]
    pub description: Option<String>,
    /// Current workflow status information
    pub status: JiraStatus,
    /// Date when the issue was resolved (ISO format if completed)
    #[serde(default)]
    pub resolutiondate: Option<String>,
    /// Issue priority (Highest / High / Medium / …)
    #[serde(default)]
    pub priority: Option<JiraPriority>,
    /// Last update timestamp from Jira (ISO-8601)
    #[serde(default)]
    pub updated: Option<String>,
    /// Custom and other fields keyed by Jira field id (e.g. `customfield_12345`).
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Workflow status; names vary by configuration and locale.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JiraStatus {
    /// Stable status id from Jira (preferred for storage / joins)
    #[serde(default, deserialize_with = "deserialize_jira_id")]
    pub id: String,
    /// Status name (e.g., "Done", "In Progress", "Решена" for Russian locale)
    pub name: String,
}

/// Accepts Jira ids as JSON string or number (`"3"` / `3`).
fn deserialize_jira_id<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::String(s)) => s,
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    })
}

/// Jira issue priority.
///
/// Classic Jira uses numeric ids where lower means higher urgency
/// (e.g. `"1"` = Highest). Used for inbox sorting.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JiraPriority {
    /// Priority display name (e.g. "High", "Highest")
    pub name: String,
    /// Numeric priority id as string (lower = more urgent in classic schemes)
    #[serde(default)]
    pub id: Option<String>,
}

/// One page of a JQL search.
#[derive(Serialize, Deserialize, Debug)]
pub struct JiraSearchResults {
    /// Index of the first issue in this page
    #[serde(default, rename = "startAt")]
    pub start_at: u32,
    /// Requested page size
    #[serde(default, rename = "maxResults")]
    pub max_results: u32,
    /// Total matching issues across all pages
    #[serde(default)]
    pub total: u32,
    /// Array of issues matching the search criteria
    pub issues: Vec<JiraIssue>,
}

/// Jira client with cached-session management via the [`Session`] trait.
#[derive(Debug)]
pub struct Jira {
    client: Client,
    config: JiraConfig,
    /// Held in memory only while authenticating.
    credentials: Option<LoginCredentials>,
    retries: i32,
}

impl Session for Jira {
    /// Logs in and returns the session cookie as `name=value`.
    async fn login(&self) -> Result<String> {
        let credentials = self.credentials.clone().expect("Credentials not set!");

        let auth_url = format!("{}/{}", self.config.api_url, AUTH_URL);
        let auth_res = self.client.post(auth_url).json(&credentials).send().await?;

        if !auth_res.status().is_success() {
            anyhow::bail!("Jira authenticate failed")
        }

        let session_res = auth_res.json::<JiraSessionResponse>().await?;

        let session_id = format!("{}={}", session_res.session.name, session_res.session.value);
        Ok(session_id)
    }

    fn set_credentials(&mut self, password: &str) -> Result<()> {
        self.credentials = Some(LoginCredentials {
            username: self.config.login.to_string(),
            password: password.to_owned(),
        });
        Ok(())
    }

    fn session_id_file(&self) -> &str {
        SESSION_ID_FILE
    }

    fn secret(&self) -> Secret {
        Secret::new(SECRET_FILE, "Enter your Jira password")
    }

    fn retry(&self) -> i32 {
        self.retries
    }

    fn inc_retry(&mut self) {
        self.retries += 1;
    }

    fn reset_retry(&mut self) {
        self.retries = 0;
    }
}

impl Jira {
    /// Builds a client from the config; no network activity yet.
    ///
    /// ```rust,no_run
    /// use kasl::api::jira::{Jira, JiraConfig};
    ///
    /// let config = JiraConfig {
    ///     login: "username".to_string(),
    ///     api_url: "https://jira.company.com".to_string(),
    ///     completed_statuses: Vec::new(),
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

    /// Fetches the user's issues resolved on `date` (full-day range).
    ///
    /// A 401 drops the cached session and retries up to the limit; other
    /// failures propagate - completed issues feed the report, so a silent
    /// empty result would hide real problems.
    ///
    /// ```rust,no_run
    /// # use kasl::api::jira::{Jira, JiraConfig};
    /// # use chrono::NaiveDate;
    /// # use anyhow::Result;
    /// # async fn example() -> Result<()> {
    /// let config = JiraConfig {
    ///     login: "username".to_string(),
    ///     api_url: "https://jira.company.com".to_string(),
    ///     completed_statuses: Vec::new(),
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
        let mut local_retries = 0;
        loop {
            let session_id = self.get_session_id().await?;

            match self.fetch_completed_pages(&session_id, date).await {
                Ok(issues) => return Ok(issues),
                Err(SearchPageError::Unauthorized) if local_retries < MAX_RETRY_COUNT => {
                    let _ = self.delete_session_id();
                    local_retries += 1;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(SearchPageError::Unauthorized) => {
                    anyhow::bail!("Jira session unauthorized after retries")
                }
                Err(SearchPageError::Other(msg)) => {
                    anyhow::bail!("Jira completed-issues search failed: {msg}")
                }
            }
        }
    }

    /// Fetches all pages of completed issues for a valid session cookie.
    async fn fetch_completed_pages(&self, session_id: &str, date: &NaiveDate) -> std::result::Result<Vec<JiraIssue>, SearchPageError> {
        // Filter by resolution date only. Do not use `status in (...)` with English
        // defaults like "Done"/"Resolved": on localized Jira those names are invalid
        // and the whole JQL fails (HTTP 400), which used to look like "no issues".
        let date_str = date.format("%Y-%m-%d").to_string();
        let jql = format!(
            "assignee = currentUser() AND resolved >= \"{}\" AND resolved <= \"{} 23:59\"",
            date_str, date_str
        );
        let url = format!("{}/{}", self.config.api_url, SEARCH_URL);

        let mut all = Vec::new();
        let mut start_at: u32 = 0;

        loop {
            let mut headers = HeaderMap::new();
            headers.insert(
                COOKIE,
                HeaderValue::from_str(session_id).map_err(|e| SearchPageError::Other(format!("invalid session cookie: {e}")))?,
            );

            let res = self
                .client
                .get(&url)
                .headers(headers)
                .query(&[
                    ("jql", jql.as_str()),
                    ("fields", "summary,status,priority,updated,resolutiondate"),
                    ("startAt", &start_at.to_string()),
                    ("maxResults", &SEARCH_PAGE_SIZE.to_string()),
                ])
                .send()
                .await
                .map_err(|e| SearchPageError::Other(format!("request failed: {e}")))?;

            match res.status() {
                StatusCode::UNAUTHORIZED => return Err(SearchPageError::Unauthorized),
                status if !status.is_success() => {
                    let body = res.text().await.unwrap_or_default();
                    return Err(SearchPageError::Other(format!("HTTP {status}: {body}")));
                }
                _ => {}
            }

            let page: JiraSearchResults = res.json().await.map_err(|e| SearchPageError::Other(format!("invalid JSON: {e}")))?;
            let batch_len = page.issues.len() as u32;
            all.extend(page.issues);

            start_at += batch_len;
            if batch_len == 0 || start_at >= page.total {
                break;
            }
        }

        Ok(all)
    }

    /// Fetches open issues currently assigned to the authenticated user.
    ///
    /// Uses JQL `assignee = currentUser() AND resolution is EMPTY`. Paginates
    /// through all matching issues (`startAt` / `total`). Extra field ids
    /// (custom fields such as Scoring) are included in the `fields` query.
    ///
    /// Auth failures and network errors return an empty list (same pattern as
    /// [`get_completed_issues`]) so callers can keep polling safely.
    pub async fn get_assigned_open_issues(&mut self, extra_field_ids: &[String]) -> Result<Vec<JiraIssue>> {
        let mut local_retries = 0;
        loop {
            let session_id = match self.get_session_id().await {
                Ok(id) => id,
                Err(_) => return Ok(Vec::new()),
            };

            match self.fetch_assigned_open_pages(&session_id, extra_field_ids).await {
                Ok(issues) => return Ok(issues),
                Err(SearchPageError::Unauthorized) if local_retries < MAX_RETRY_COUNT => {
                    let _ = self.delete_session_id();
                    local_retries += 1;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(_) => return Ok(Vec::new()),
            }
        }
    }

    /// Like [`get_assigned_open_issues`], but never prompts for a password.
    ///
    /// Uses a cached session cookie and/or encrypted `.jira_secret`. Returns
    /// `Ok(None)` when neither is available so background daemons can skip
    /// the poll without blocking on stdin.
    pub async fn get_assigned_open_issues_noninteractive(&mut self, extra_field_ids: &[String]) -> Result<Option<Vec<JiraIssue>>> {
        let mut local_retries = 0;
        loop {
            let Some(session_id) = self.session_id_noninteractive().await? else {
                return Ok(None);
            };

            match self.fetch_assigned_open_pages(&session_id, extra_field_ids).await {
                Ok(issues) => return Ok(Some(issues)),
                Err(SearchPageError::Unauthorized) if local_retries < MAX_RETRY_COUNT => {
                    let _ = self.delete_session_id();
                    local_retries += 1;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(_) => return Ok(Some(Vec::new())),
            }
        }
    }

    /// Fetches all pages of assigned open issues for a valid session cookie.
    async fn fetch_assigned_open_pages(&self, session_id: &str, extra_field_ids: &[String]) -> std::result::Result<Vec<JiraIssue>, SearchPageError> {
        let jql = "assignee = currentUser() AND resolution is EMPTY ORDER BY priority ASC, updated DESC";
        let fields = build_search_fields(extra_field_ids);
        let url = format!("{}/{}", self.config.api_url, SEARCH_URL);

        let mut all = Vec::new();
        let mut start_at: u32 = 0;

        loop {
            let mut headers = HeaderMap::new();
            headers.insert(
                COOKIE,
                HeaderValue::from_str(session_id).map_err(|e| SearchPageError::Other(format!("invalid session cookie: {e}")))?,
            );

            let res = self
                .client
                .get(&url)
                .headers(headers)
                .query(&[
                    ("jql", jql),
                    ("fields", fields.as_str()),
                    ("startAt", &start_at.to_string()),
                    ("maxResults", &SEARCH_PAGE_SIZE.to_string()),
                ])
                .send()
                .await
                .map_err(|e| SearchPageError::Other(format!("request failed: {e}")))?;

            match res.status() {
                StatusCode::UNAUTHORIZED => return Err(SearchPageError::Unauthorized),
                status if !status.is_success() => {
                    let body = res.text().await.unwrap_or_default();
                    return Err(SearchPageError::Other(format!("HTTP {status}: {body}")));
                }
                _ => {}
            }

            let page: JiraSearchResults = res.json().await.map_err(|e| SearchPageError::Other(format!("invalid JSON: {e}")))?;
            let batch_len = page.issues.len() as u32;
            all.extend(page.issues);

            start_at += batch_len;
            if batch_len == 0 || start_at >= page.total {
                break;
            }
        }

        Ok(all)
    }

    /// Resolves a session from cache / secret without prompting.
    async fn session_id_noninteractive(&mut self) -> Result<Option<String>> {
        let session_id_file_path = crate::libs::data_storage::DataStorage::new().get_path(SESSION_ID_FILE)?;
        let path_str = session_id_file_path.to_str().unwrap_or_default();

        if let Ok(session_id) = Self::read_session_id(path_str) {
            return Ok(Some(session_id));
        }

        let Some(password) = self.secret().try_get_cached() else {
            return Ok(None);
        };

        self.set_credentials(&password)?;
        match self.login().await {
            Ok(session_id) => {
                let _ = Self::write_session_id(path_str, &session_id);
                self.reset_retry();
                Ok(Some(session_id))
            }
            Err(_) => Ok(None),
        }
    }

    /// Builds a browse URL for an issue key using this client's API base.
    pub fn issue_browse_url(&self, key: &str) -> String {
        let base = self.config.api_url.trim_end_matches('/');
        format!("{}/browse/{}", base, key)
    }

    /// Maps a Jira priority id to a sortable rank (lower = more urgent).
    pub fn priority_rank(priority: &Option<JiraPriority>) -> i32 {
        priority
            .as_ref()
            .and_then(|p| p.id.as_ref())
            .and_then(|id| id.parse::<i32>().ok())
            .unwrap_or(999)
    }

    /// Extracts a numeric value from a Jira custom-field JSON value.
    ///
    /// Supports bare numbers, numeric strings, and objects with `value` / `amount`.
    pub fn extract_number(value: &Value) -> Option<f64> {
        match value {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse().ok(),
            Value::Object(map) => map
                .get("value")
                .and_then(Self::extract_number)
                .or_else(|| map.get("amount").and_then(Self::extract_number)),
            _ => None,
        }
    }

    /// Reads a numeric custom field from issue extras by field id.
    pub fn sort_value_from_issue(issue: &JiraIssue, field_id: &str) -> Option<f64> {
        issue.fields.extra.get(field_id).and_then(Self::extract_number)
    }
}

/// Internal error for paginated search (auth vs other failures).
enum SearchPageError {
    Unauthorized,
    Other(String),
}

fn build_search_fields(extra_field_ids: &[String]) -> String {
    let mut fields = vec!["summary".to_string(), "status".to_string(), "priority".to_string(), "updated".to_string()];
    for id in extra_field_ids {
        let trimmed = id.trim();
        if !trimmed.is_empty() && !fields.iter().any(|f| f == trimmed) {
            fields.push(trimmed.to_string());
        }
    }
    fields.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_number_from_primitives_and_objects() {
        assert_eq!(Jira::extract_number(&json!(12.5)), Some(12.5));
        assert_eq!(Jira::extract_number(&json!("42")), Some(42.0));
        assert_eq!(Jira::extract_number(&json!({"value": 7})), Some(7.0));
        assert_eq!(Jira::extract_number(&json!({"amount": "3.5"})), Some(3.5));
        assert_eq!(Jira::extract_number(&json!(null)), None);
    }

    #[test]
    fn build_search_fields_includes_custom_ids() {
        let fields = build_search_fields(&["customfield_10001".to_string(), "summary".to_string()]);
        assert!(fields.contains("summary"));
        assert!(fields.contains("customfield_10001"));
        assert_eq!(fields.matches("summary").count(), 1);
    }
}

/// Jira connection settings; passwords are never stored here.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JiraConfig {
    /// Username (email for Atlassian Cloud accounts).
    pub login: String,

    /// Instance root URL, without the `/rest/api/` path.
    pub api_url: String,

    /// Deprecated: previously used in `status in (...)` for completed-issue search.
    ///
    /// Kept for config compatibility. Discovery now filters by `resolved` date only,
    /// because non-existent status names (e.g. English "Done" on a Russian Jira)
    /// make the whole JQL fail.
    #[serde(default = "default_completed_statuses")]
    pub completed_statuses: Vec<String>,
}

/// Empty default — status names are no longer used in completed-issue JQL.
fn default_completed_statuses() -> Vec<String> {
    Vec::new()
}

impl JiraConfig {
    /// Module metadata for the setup wizard.
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "jira".to_string(),
            name: "Jira".to_string(),
        }
    }

    /// Interactive setup; existing values become the prompt defaults.
    ///
    /// ```rust,no_run
    /// # use kasl::api::JiraConfig;
    /// # use anyhow::Result;
    /// # fn example() -> Result<()> {
    /// let existing_config = Some(JiraConfig {
    ///     login: "olduser".to_string(),
    ///     api_url: "https://old-jira.com".to_string(),
    ///     completed_statuses: Vec::new(),
    /// });
    ///
    /// let new_config = JiraConfig::init(&existing_config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn init(config: &Option<Self>) -> Result<Self> {
        // Use existing configuration as defaults, or create empty defaults
        let config = config.clone().unwrap_or(Self {
            login: "".to_string(),
            api_url: "".to_string(),
            completed_statuses: default_completed_statuses(),
        });

        // Display configuration module header
        msg_print!(Message::ConfigModuleJira);

        // Interactive configuration with existing values as defaults
        Ok(Self {
            completed_statuses: config.completed_statuses.clone(),
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
