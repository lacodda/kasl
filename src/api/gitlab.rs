use crate::libs::config::GitLabConfig;
use chrono::{Duration, Local};
use reqwest::Client;
use serde::Deserialize;

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
    commit_to: String,
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
                    let commit_detail = self.get_commit_detail(event.project_id, &push_data.commit_to).await?;
                    commits_info.push(CommitInfo {
                        sha: commit_detail.id,
                        message: commit_detail.message,
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
