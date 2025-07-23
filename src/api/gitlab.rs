//! Provides a client for interacting with the GitLab API.
//!
//! This module is responsible for fetching user-specific data from GitLab,
//! such as daily commits, which can then be used to populate task lists
//! within the application.

use crate::libs::config::ConfigModule;
use crate::libs::messages::Message;
use crate::{msg_error, msg_print};
use anyhow::Result;
use chrono::{Duration, Local};
use dialoguer::{theme::ColorfulTheme, Input};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// A client for the GitLab API.
#[derive(Debug)]
pub struct GitLab {
    /// The `reqwest` client used for making HTTP requests.
    client: Client,
    /// The configuration specific to the GitLab API.
    config: GitLabConfig,
}

/// Represents a GitLab API event.
#[derive(Debug, Deserialize)]
struct Event {
    action_name: String,
    push_data: Option<PushData>,
    project_id: u32,
}

/// Contains data related to a "push" event.
#[derive(Debug, Deserialize)]
struct PushData {
    commit_to: Option<String>,
}

/// A simplified representation of a GitLab commit.
#[derive(Debug)]
pub struct CommitInfo {
    /// The SHA hash of the commit.
    pub sha: String,
    /// The commit message.
    pub message: String,
}

/// Represents a commit object from the GitLab API.
#[derive(Debug, Deserialize)]
struct Commit {
    id: String,
    message: String,
}

/// Represents a user object from the GitLab API, used to get the user ID.
#[derive(Debug, Deserialize)]
struct User {
    id: u32,
}

impl GitLab {
    /// Creates a new `GitLab` client instance.
    ///
    /// # Arguments
    ///
    /// * `config` - A reference to the `GitLabConfig` containing the API URL and access token.
    pub fn new(config: &GitLabConfig) -> Self {
        Self {
            client: Client::new(),
            config: config.clone(),
        }
    }

    /// Fetches the current user's ID from the GitLab API.
    pub async fn get_user_id(&self) -> Result<u32> {
        let url = format!("{}/api/v4/user", self.config.api_url);
        let response = self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await?;

        Ok(response.json::<User>().await?.id)
    }

    /// Fetches all commits made by the user today.
    ///
    /// This function is designed to be resilient to network errors. If an API
    /// request or parsing fails, it logs the error to `stderr` and returns
    /// an empty `Vec`, preventing the application from crashing.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<CommitInfo>` on success, or an empty `Vec` if
    /// an error occurs.
    pub async fn get_today_commits(&self) -> Result<Vec<CommitInfo>> {
        let today = Local::now();
        let yesterday = (today - Duration::days(1)).format("%Y-%m-%d").to_string();
        let tomorrow = (today + Duration::days(1)).format("%Y-%m-%d").to_string();

        let user_id = match self.get_user_id().await {
            Ok(id) => id,
            Err(e) => {
                msg_error!(Message::GitlabUserIdFailed(e.to_string()));
                return Ok(Vec::new());
            }
        };

        let url = format!(
            "{}/api/v4/users/{}/events?after={}&before={}",
            self.config.api_url, user_id, yesterday, tomorrow
        );

        let response = match self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await {
            Ok(res) => res,
            Err(e) => {
                msg_error!(Message::GitlabFetchFailed(e.to_string()));
                return Ok(Vec::new());
            }
        };

        let events = match response.json::<Vec<Event>>().await {
            Ok(ev) => ev,
            Err(e) => {
                msg_error!(Message::GitlabFetchFailed(e.to_string()));
                return Ok(Vec::new());
            }
        };

        let mut commits_info = Vec::new();
        for event in events {
            if event.action_name == "pushed to" {
                if let Some(push_data) = event.push_data {
                    if let Some(commit_to) = push_data.commit_to {
                        let commit_detail = match self.get_commit_detail(event.project_id, &commit_to).await {
                            Ok(detail) => detail,
                            Err(_) => continue, // Skip if detail fetch fails
                        };
                        commits_info.push(CommitInfo {
                            sha: commit_detail.id,
                            message: commit_detail
                                .message
                                .split_once('\n')
                                .map(|(part, _)| part)
                                .unwrap_or(&commit_detail.message)
                                .to_string(),
                        });
                    }
                }
            }
        }

        Ok(commits_info)
    }

    /// Fetches the details of a single commit by its SHA.
    async fn get_commit_detail(&self, project_id: u32, commit_sha: &str) -> Result<Commit> {
        let url = format!("{}/api/v4/projects/{}/repository/commits/{}", self.config.api_url, project_id, commit_sha);
        let response = self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await?;

        Ok(response.json::<Commit>().await?)
    }
}

/// Configuration settings for the GitLab API client.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitLabConfig {
    /// The personal access token for API authentication.
    pub access_token: String,
    /// The base URL of the GitLab instance (e.g., "https://gitlab.com").
    pub api_url: String,
}

impl GitLabConfig {
    /// Returns the configuration module descriptor for the setup wizard.
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "gitlab".to_string(),
            name: "GitLab".to_string(),
        }
    }

    /// Runs an interactive prompt to initialize the GitLab configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - An `Option` containing the existing configuration, if any.
    pub fn init(config: &Option<GitLabConfig>) -> Result<Self> {
        let config = config.clone().unwrap_or(Self {
            access_token: "".to_string(),
            api_url: "".to_string(),
        });

        msg_print!(Message::ConfigModuleGitLab);
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
