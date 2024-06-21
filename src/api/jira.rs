use crate::libs::config::ConfigModule;
use dialoguer::{theme::ColorfulTheme, Input};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Serialize, Deserialize, Debug)]
pub struct JiraIssue {
    pub id: String,
    pub key: String,
    pub fields: JiraIssueFields,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JiraIssueFields {
    pub summary: String,
    pub description: Option<String>,
    pub status: JiraStatus,
    pub resolutiondate: Option<String>, // Поле даты завершения задачи
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JiraStatus {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JiraSearchResults {
    pub issues: Vec<JiraIssue>,
}

pub struct Jira {
    client: Client,
    base_url: String,
    token: String,
}

impl Jira {
    pub fn new(base_url: &str, token: &str) -> Self {
        Jira {
            client: Client::new(),
            base_url: base_url.to_string(),
            token: token.to_string(),
        }
    }

    pub async fn get_issue(&self, issue_key: &str) -> Result<JiraIssue, Box<dyn Error>> {
        let url = format!("{}/rest/api/2/issue/{}", self.base_url, issue_key);
        let response = self.client.get(&url).bearer_auth(&self.token).send().await?;

        let issue = response.json::<JiraIssue>().await?;
        Ok(issue)
    }

    pub async fn create_issue(&self, issue: &JiraIssue) -> Result<JiraIssue, Box<dyn Error>> {
        let url = format!("{}/rest/api/2/issue", self.base_url);
        let response = self.client.post(&url).bearer_auth(&self.token).json(issue).send().await?;

        let created_issue = response.json::<JiraIssue>().await?;
        Ok(created_issue)
    }

    pub async fn get_completed_issues_by_date(&self, date: &str) -> Result<Vec<JiraIssue>, Box<dyn Error>> {
        let jql = format!("status = Done AND resolutiondate >= \"{}\" AND resolutiondate < \"{} 23:59\"", date, date);
        let url = format!("{}/rest/api/2/search?jql={}", self.base_url, jql);

        let response = self.client.get(&url).bearer_auth(&self.token).send().await?;

        let search_results = response.json::<JiraSearchResults>().await?;
        Ok(search_results.issues)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JiraConfig {
    pub access_token: String,
    pub api_url: String,
}

impl JiraConfig {
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "jira".to_string(),
            name: "Jira".to_string(),
        }
    }
    pub fn init(config: &Option<Self>) -> Result<Self, Box<dyn Error>> {
        let config = config
            .clone()
            .or(Some(Self {
                access_token: "".to_string(),
                api_url: "".to_string(),
            }))
            .unwrap();
        println!("Jira settings");
        Ok(Self {
            access_token: Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter your Jira private token")
                .default(config.access_token)
                .interact_text()?,
            api_url: Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter the Jira API URL")
                .default(config.api_url)
                .interact_text()?,
        })
    }
}
