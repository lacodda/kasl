use super::Session;
use crate::libs::{config::ConfigModule, secret::Secret};
use anyhow::Result;
use chrono::NaiveDate;
use dialoguer::{theme::ColorfulTheme, Input};
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_RETRY_COUNT: i32 = 3;
const SESSION_ID_FILE: &str = ".jira_session_id";
const SECRET_FILE: &str = ".jira_secret";
const AUTH_URL: &str = "rest/auth/1/session";
const SEARCH_URL: &str = "rest/api/2/search";

#[derive(Serialize, Clone, Debug)]
pub struct LoginCredentials {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct JiraSessionResponse {
    session: JiraSession,
}

#[derive(Serialize, Deserialize, Debug)]
struct JiraSession {
    name: String,
    value: String,
}

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
    pub resolutiondate: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JiraStatus {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JiraSearchResults {
    pub issues: Vec<JiraIssue>,
}

#[derive(Debug)]
pub struct Jira {
    client: Client,
    config: JiraConfig,
    credentials: Option<LoginCredentials>,
    retries: i32,
}

impl Session for Jira {
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
}

impl Jira {
    pub fn new(config: &JiraConfig) -> Self {
        Self {
            client: Client::new(),
            config: config.clone(),
            credentials: None,
            retries: 0,
        }
    }

    pub async fn get_completed_issues(&mut self, date: &NaiveDate) -> Result<Vec<JiraIssue>> {
        loop {
            let session_id = match self.get_session_id().await {
                Ok(id) => id,
                Err(_) => return Ok(Vec::new()),
            };

            let date = date.format("%Y-%m-%d").to_string();
            let jql = format!(
                "status in (Done, Решена) AND resolved >= \"{}\" AND resolved <= \"{} 23:59\" AND assignee in (currentUser())",
                &date, &date
            );

            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&session_id)?);
            let url = format!("{}/{}?jql={}", &self.config.api_url, SEARCH_URL, &jql);

            let res = match self.client.get(&url).headers(headers).send().await {
                Ok(response) => response,
                Err(_) => return Ok(Vec::new()),
            };

            match res.status() {
                StatusCode::UNAUTHORIZED if self.retries < MAX_RETRY_COUNT => {
                    self.delete_session_id()?;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                _ => {
                    let search_results = res.json::<JiraSearchResults>().await?;
                    return Ok(search_results.issues);
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JiraConfig {
    pub login: String,
    pub api_url: String,
}

impl JiraConfig {
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "jira".to_string(),
            name: "Jira".to_string(),
        }
    }
    pub fn init(config: &Option<Self>) -> Result<Self> {
        let config = config
            .clone()
            .or(Some(Self {
                login: "".to_string(),
                api_url: "".to_string(),
            }))
            .unwrap();
        println!("Jira settings");
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
