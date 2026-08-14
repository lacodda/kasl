//! GitLab API client for fetching user activity and commit data.
//!
//! Provides integration with GitLab instances (both self-hosted and GitLab.com)
//! to automatically discover and import development activities as tasks.
//!
//! ## Usage
//!
//! ```rust,no_run
//! # use kasl::api::gitlab::{GitLab, GitLabConfig};
//! # async fn f() -> anyhow::Result<()> {
//! let config = GitLabConfig {
//!     access_token: "glpat-xxxxxxxxxxxxxxxxxxxx".to_string(),
//!     api_url: "https://gitlab.com".to_string(),
//! };
//!
//! let client = GitLab::new(&config);
//! let commits = client.get_today_commits().await?;
//! # Ok(())
//! # }
//! ```

use crate::libs::config::ConfigModule;
use crate::libs::messages::Message;
use crate::{msg_error, msg_print};
use anyhow::Result;
use chrono::{DateTime, Duration, Local, NaiveDate};
use dialoguer::{Input, theme::ColorfulTheme};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// GitLab client; stateless - the token rides on every request.
#[derive(Debug)]
pub struct GitLab {
    client: Client,
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
/// metadata about the push operation. Only `commit_to` (the tip) is enough for
/// single-commit pushes; multi-commit pushes also expose `commit_from` and
/// `commit_count` so the full range can be expanded via the compare API.
#[derive(Debug, Deserialize)]
struct PushData {
    /// SHA of the tip commit after the push
    commit_to: Option<String>,
    /// SHA of the tip before the push (exclusive start of the pushed range)
    commit_from: Option<String>,
    /// Number of commits included in the push
    commit_count: Option<u32>,
}

/// Response from GitLab's repository compare API.
#[derive(Debug, Deserialize)]
struct CompareResult {
    commits: Vec<Commit>,
}

/// Simplified commit information for task creation.
#[derive(Debug)]
pub struct CommitInfo {
    /// Full SHA hash of the commit for unique identification
    pub sha: String,
    /// First line of the commit message (typically the summary)
    pub message: String,
}

/// Detailed commit object returned by GitLab's commits / compare APIs.
#[derive(Debug, Deserialize)]
struct Commit {
    /// Full SHA identifier of the commit
    id: String,
    /// Complete commit message including body and trailers
    message: String,
    /// Author email (used to keep only the current user's commits)
    author_email: Option<String>,
    /// Author display name (fallback when email is missing)
    author_name: Option<String>,
    /// When the commit was authored (ISO-8601)
    authored_date: Option<String>,
    /// When the commit was committed (fallback date filter)
    committed_date: Option<String>,
}

/// GitLab user information for the authenticated account.
#[derive(Debug, Deserialize)]
struct User {
    /// Numeric user identifier in GitLab
    id: u32,
    /// Primary email from `/user` (best match for commit author_email)
    email: Option<String>,
    /// Display name (fallback author match)
    name: Option<String>,
}

impl GitLab {
    /// Builds a client from the config; no network activity yet.
    ///
    /// ```rust,no_run
    /// # use kasl::api::gitlab::{GitLab, GitLabConfig};
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

    /// The authenticated user's numeric id (`GET /user`, `read_user` scope).
    pub async fn get_user_id(&self) -> Result<u32> {
        Ok(self.get_current_user().await?.id)
    }

    /// Fetches the authenticated GitLab user (`/user`).
    async fn get_current_user(&self) -> Result<User> {
        let url = format!("{}/api/v4/user", self.config.api_url);
        let response = self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await?;

        Ok(response.json::<User>().await?)
    }

    /// Today's commits by the authenticated user, deduplicated across
    /// pushes, first message line only - the feed for task discovery.
    pub async fn get_today_commits(&self) -> Result<Vec<CommitInfo>> {
        // Events window is wider (yesterday..tomorrow) to absorb timezone skew;
        // individual commits are then filtered to local today + current author.
        let today = Local::now();
        let today_date = today.date_naive();
        let yesterday = (today - Duration::days(1)).format("%Y-%m-%d").to_string();
        let tomorrow = (today + Duration::days(1)).format("%Y-%m-%d").to_string();

        let user = self.get_current_user().await.inspect_err(|e| {
            msg_error!(Message::GitlabUserIdFailed(e.to_string()));
        })?;

        let events = self.fetch_user_events(user.id, &yesterday, &tomorrow).await?;

        // Expand each push to all commits in the range (not only the tip).
        let mut commits_info = Vec::new();
        let mut seen_shas = HashSet::new();

        for event in events {
            if !matches!(event.action_name.as_str(), "pushed to" | "pushed new") {
                continue;
            }
            let Some(push_data) = event.push_data else {
                continue;
            };

            let commits = match self.commits_for_push(event.project_id, &push_data).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            for commit in commits {
                if !is_commit_by_user(&commit, &user) {
                    continue;
                }
                if !is_commit_on_date(&commit, today_date) {
                    continue;
                }
                if !seen_shas.insert(commit.id.clone()) {
                    continue;
                }
                let clean_message = commit.message.split_once('\n').map(|(part, _)| part).unwrap_or(&commit.message).to_string();

                commits_info.push(CommitInfo {
                    sha: commit.id,
                    message: clean_message,
                });
            }
        }

        Ok(commits_info)
    }

    /// Fetches all pages of user events in the given date window.
    async fn fetch_user_events(&self, user_id: u32, after: &str, before: &str) -> Result<Vec<Event>> {
        let mut all = Vec::new();
        let mut page: u32 = 1;

        loop {
            let url = format!("{}/api/v4/users/{}/events", self.config.api_url, user_id);
            let response = self
                .client
                .get(&url)
                .header("PRIVATE-TOKEN", &self.config.access_token)
                .query(&[("after", after), ("before", before), ("per_page", "100"), ("page", &page.to_string())])
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("GitLab events request failed: HTTP {status}: {body}");
            }

            let batch: Vec<Event> = response.json().await?;
            let batch_len = batch.len();
            all.extend(batch);

            if batch_len < 100 {
                break;
            }
            page += 1;
        }

        Ok(all)
    }

    /// Returns every commit included in a push event.
    ///
    /// Single-commit pushes use the tip (`commit_to`). Multi-commit pushes use
    /// GitLab's compare API between `commit_from` and `commit_to`, otherwise only
    /// the merge/tip message would be visible to task discovery.
    async fn commits_for_push(&self, project_id: u32, push: &PushData) -> Result<Vec<Commit>> {
        let Some(commit_to) = push.commit_to.as_deref() else {
            return Ok(Vec::new());
        };

        let count = push.commit_count.unwrap_or(1);
        if count > 1
            && let Some(commit_from) = push.commit_from.as_deref()
            && !is_null_sha(commit_from)
            && commit_from != commit_to
        {
            match self.compare_commits(project_id, commit_from, commit_to).await {
                Ok(commits) if !commits.is_empty() => return Ok(commits),
                _ => {}
            }
        }

        Ok(vec![self.get_commit_detail(project_id, commit_to).await?])
    }

    /// Fetches commits in `(from, to]` via the repository compare API.
    async fn compare_commits(&self, project_id: u32, from: &str, to: &str) -> Result<Vec<Commit>> {
        let url = format!("{}/api/v4/projects/{}/repository/compare", self.config.api_url, project_id);
        let response = self
            .client
            .get(&url)
            .header("PRIVATE-TOKEN", &self.config.access_token)
            .query(&[("from", from), ("to", to)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("GitLab compare failed: HTTP {status}: {body}");
        }

        Ok(response.json::<CompareResult>().await?.commits)
    }

    /// Fetches one commit's full object from the commits API.
    async fn get_commit_detail(&self, project_id: u32, commit_sha: &str) -> Result<Commit> {
        let url = format!("{}/api/v4/projects/{}/repository/commits/{}", self.config.api_url, project_id, commit_sha);
        let response = self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await?;

        Ok(response.json::<Commit>().await?)
    }
}

/// Returns true for GitLab's all-zero "no parent" SHA used on new branches.
fn is_null_sha(sha: &str) -> bool {
    !sha.is_empty() && sha.bytes().all(|b| b == b'0')
}

/// True when the commit author matches the authenticated GitLab user.
///
/// Prefers email (stable across display-name spelling); falls back to name.
fn is_commit_by_user(commit: &Commit, user: &User) -> bool {
    if let (Some(commit_email), Some(user_email)) = (&commit.author_email, &user.email)
        && !user_email.is_empty()
        && commit_email.eq_ignore_ascii_case(user_email)
    {
        return true;
    }

    if let (Some(commit_name), Some(user_name)) = (&commit.author_name, &user.name)
        && !user_name.is_empty()
        && commit_name.eq_ignore_ascii_case(user_name)
    {
        return true;
    }

    false
}

/// True when the commit was authored on `date` (local TZ).
///
/// Falls back to `committed_date` only when `authored_date` is missing.
fn is_commit_on_date(commit: &Commit, date: NaiveDate) -> bool {
    let raw = commit.authored_date.as_deref().or(commit.committed_date.as_deref());
    let Some(raw) = raw else {
        return false;
    };
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Local).date_naive() == date)
        .unwrap_or(false)
}

/// GitLab connection settings. The token IS stored in the config file -
/// keep its scopes minimal (`read_user`, `read_repository`).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitLabConfig {
    /// Personal Access Token with `read_user` + `read_repository` scopes.
    pub access_token: String,

    /// Instance root URL, without the `/api/v4` path.
    pub api_url: String,
}

impl GitLabConfig {
    /// Module metadata for the setup wizard.
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "gitlab".to_string(),
            name: "GitLab".to_string(),
        }
    }

    /// Interactive setup; existing values become the prompt defaults.
    ///
    /// ```rust,no_run
    /// # use kasl::api::gitlab::GitLabConfig;
    /// # fn f() -> anyhow::Result<()> {
    /// let existing_config = Some(GitLabConfig {
    ///     access_token: "glpat-old-token".to_string(),
    ///     api_url: "https://gitlab.com".to_string(),
    /// });
    ///
    /// let new_config = GitLabConfig::init(&existing_config)?;
    /// # Ok(())
    /// # }
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
