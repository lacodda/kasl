use crate::libs::{config::ConfigModule, data_storage::DataStorage, secret::Secret};
use chrono::NaiveDate;
use dialoguer::{theme::ColorfulTheme, Input};
use reqwest::{
    header::{HeaderMap, HeaderValue, COOKIE},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs,
    io::{self, Write},
    time::Duration,
};

const MAX_RETRY_COUNT: i32 = 3;
const SESSION_ID_FILE: &str = ".jira_session_id";
const SECRET_FILE: &str = ".jira_secret";
const AUTH_URL: &str = "rest/auth/1/session";
const SEARCH_URL: &str = "rest/api/2/search";

#[derive(Serialize)]
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
    secret: Secret,
    retries: i32,
}

impl Jira {
    pub fn new(config: &JiraConfig) -> Self {
        Self {
            client: Client::new(),
            config: config.clone(),
            secret: Secret::new(SECRET_FILE, "Enter your Jira password"),
            retries: 0,
        }
    }

    pub async fn get_completed_issues(&mut self, date: &NaiveDate) -> Result<Vec<JiraIssue>, Box<dyn Error>> {
        loop {
            let session_id = self.get_session_id().await?;
            let date = date.format("%Y-%m-%d").to_string();
            let jql = format!(
                "status in (Done, Решена) AND resolved >= \"{}\" AND resolved <= \"{} 23:59\" AND assignee in (currentUser())",
                &date, &date
            );

            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&session_id)?);
            let url = format!("{}/{}?jql={}", &self.config.api_url, SEARCH_URL, &jql);

            let res = self.client.get(&url).headers(headers).send().await?;

            match res.status() {
                StatusCode::UNAUTHORIZED if self.retries < MAX_RETRY_COUNT => {
                    self.delete_session_id()?;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    self.retries += 1;
                    continue;
                }
                _ => {
                    let search_results = res.json::<JiraSearchResults>().await?;
                    return Ok(search_results.issues);
                }
            }
        }
    }

    pub async fn login(&self, credentials: &LoginCredentials) -> Result<String, Box<dyn Error>> {
        let auth_url = format!("{}/{}", self.config.api_url, AUTH_URL);
        let auth_res = self.client.post(auth_url).json(credentials).send().await?;

        if !auth_res.status().is_success() {
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Jira authenticate failed")));
        }

        let session_res = auth_res.json::<JiraSessionResponse>().await?;
        let session_id = format!("{}={}", session_res.session.name, session_res.session.value);
        Ok(session_id)
    }

    async fn get_session_id(&mut self) -> Result<String, Box<dyn Error>> {
        let session_id_file_path = DataStorage::new().get_path(SESSION_ID_FILE)?;
        let session_id_file_path_str = session_id_file_path.to_str().unwrap();
        if let Ok(session_id) = Self::read_session_id(&session_id_file_path_str) {
            return Ok(session_id);
        } else {
            loop {
                let password: String = match self.retries > 0 {
                    true => self.secret.prompt()?,
                    false => self.secret.get_or_prompt()?,
                };
                let login_credentials = LoginCredentials {
                    username: self.config.login.to_string(),
                    password: password,
                };
                let session_id = self.login(&login_credentials).await;
                match session_id {
                    Ok(session_id) => {
                        let _ = Self::write_session_id(&session_id_file_path_str, &session_id);
                        return Ok(session_id);
                    }
                    Err(_) => {
                        if self.retries < MAX_RETRY_COUNT {
                            self.retries += 1;
                            continue;
                        }
                        break Err(format!("You entered the wrong password {} times!", MAX_RETRY_COUNT).into());
                    }
                }
            }
        }
    }

    fn read_session_id(file_name: &str) -> io::Result<String> {
        fs::read_to_string(file_name)
    }

    fn write_session_id(file_name: &str, session_id: &str) -> io::Result<()> {
        let mut file = fs::OpenOptions::new().write(true).create(true).truncate(true).open(file_name)?;
        file.write_all(session_id.as_bytes())
    }

    fn delete_session_id(&self) -> Result<(), Box<dyn Error>> {
        let session_id_file_path = DataStorage::new().get_path(SESSION_ID_FILE)?;
        fs::remove_file(session_id_file_path)?;
        Ok(())
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
    pub fn init(config: &Option<Self>) -> Result<Self, Box<dyn Error>> {
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
