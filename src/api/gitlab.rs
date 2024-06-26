use crate::libs::config::ConfigModule;
use chrono::{Duration, Local};
use dialoguer::{theme::ColorfulTheme, Input};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug)]
pub struct GitLab {
    client: Client,
    config: GitLabConfig,
}

#[derive(Debug, Deserialize)]
struct Event {
    action_name: String,
    push_data: Option<PushData>,
    project_id: u32,
}

#[derive(Debug, Deserialize)]
struct PushData {
    commit_to: Option<String>,
}

#[derive(Debug)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct Commit {
    id: String,
    message: String,
}

#[derive(Debug, Deserialize)]
struct User {
    id: u32,
}

impl GitLab {
    pub fn new(config: &GitLabConfig) -> Self {
        Self {
            client: Client::new(),
            config: config.clone(),
        }
    }

    pub async fn get_user_id(&self) -> Result<u32, reqwest::Error> {
        let url = format!("{}/api/v4/user", self.config.api_url);
        let response = self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await?;

        Ok(response.json::<User>().await?.id)
    }

    pub async fn get_today_commits(&self) -> Result<Vec<CommitInfo>, reqwest::Error> {
        let today = Local::now();
        let yesterday = (today - Duration::days(1)).format("%Y-%m-%d").to_string();
        let tomorrow = (today + Duration::days(1)).format("%Y-%m-%d").to_string();
        let user_id = self.get_user_id().await?;
        let url = format!(
            "{}/api/v4/users/{}/events?after={}&before={}",
            self.config.api_url, user_id, yesterday, tomorrow
        );
        let response = self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await?;
        let mut commits_info = Vec::new();

        for event in response.json::<Vec<Event>>().await? {
            if event.action_name == "pushed to" {
                if let Some(push_data) = event.push_data {
                    let commit_detail = self.get_commit_detail(event.project_id, &push_data.commit_to.unwrap()).await?;
                    commits_info.push(CommitInfo {
                        sha: commit_detail.id,
                        message: commit_detail
                            .message
                            .split_once("\n")
                            .map(|(part, _)| part)
                            .unwrap_or(&commit_detail.message)
                            .to_string(),
                    });
                }
            }
        }

        Ok(commits_info)
    }

    async fn get_commit_detail(&self, project_id: u32, commit_sha: &str) -> Result<Commit, reqwest::Error> {
        let url = format!("{}/api/v4/projects/{}/repository/commits/{}", self.config.api_url, project_id, commit_sha);
        let response = self.client.get(&url).header("PRIVATE-TOKEN", &self.config.access_token).send().await?;

        Ok(response.json::<Commit>().await?)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GitLabConfig {
    pub access_token: String,
    pub api_url: String,
}

impl GitLabConfig {
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "gitlab".to_string(),
            name: "GitLab".to_string(),
        }
    }
    pub fn init(config: &Option<GitLabConfig>) -> Result<Self, Box<dyn Error>> {
        let config = config
            .clone()
            .or(Some(Self {
                access_token: "".to_string(),
                api_url: "".to_string(),
            }))
            .unwrap();
        println!("GitLab settings");
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
