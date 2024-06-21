use crate::libs::{config::ConfigModule, data_storage::DataStorage};
use base64::prelude::*;
use chrono::NaiveDate;
use dialoguer::{theme::ColorfulTheme, Input, Password};
use reqwest::{
    header::{self, HeaderMap, HeaderValue, COOKIE},
    multipart, Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    error::Error,
    fs,
    io::{self, Write},
    time::Duration,
};

const MAX_RETRY_COUNT: i32 = 3;
const COOKIE_KEY: &str = "PORTALSESSID=";
const SESSION_ID_FILE: &str = ".session_id";
const AUTH_URL: &str = "auth/ldap";
const LOGIN_URL: &str = "auth/login-by-token";
const REPORT_URL: &str = "report-card/send-daily-report";
const REST_DATES_URL: &str = "report-card/get-rest-dates";

#[derive(Serialize)]
pub struct LoginCredentials {
    login: String,
    password: String,
}

#[derive(Deserialize)]
pub struct AuthSession {
    payload: AuthPayload,
}

#[derive(Deserialize)]
pub struct AuthPayload {
    token: String,
}

#[derive(Debug, Deserialize)]
pub struct RestDatesResponse {
    dates: Vec<String>,
    v_dates: Vec<String>,
    w_dates: Vec<String>,
}

impl RestDatesResponse {
    pub fn unique_dates(&self) -> Result<HashSet<NaiveDate>, Box<dyn Error>> {
        let mut date_set = HashSet::new();

        self.process_dates(&self.dates, &mut date_set)?;
        self.process_dates(&self.v_dates, &mut date_set)?;
        self.process_dates(&self.w_dates, &mut date_set)?;

        Ok(date_set)
    }

    fn process_dates(&self, dates: &Vec<String>, date_set: &mut HashSet<NaiveDate>) -> Result<(), Box<dyn Error>> {
        dates
            .iter()
            .filter_map(|date_str| NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok())
            .for_each(|date| {
                date_set.insert(date);
            });
        Ok(())
    }
}

pub struct Si {
    client: Client,
    config: SiConfig,
}

impl Si {
    pub fn new(config: &SiConfig) -> Self {
        Self {
            client: Client::new(),
            config: config.clone(),
        }
    }

    pub async fn send(&self, data: String, date: NaiveDate) -> Result<StatusCode, Box<dyn Error>> {
        let mut retries = 0;
        loop {
            let session_id = self.get_session_id().await?;
            let url = format!("{}/{}", self.config.api_url, REPORT_URL);
            let date = date.format("%Y-%m-%d").to_string();
            let form = multipart::Form::new()
                .text("date", date)
                .text("tasks", data.clone())
                .text("comment", "")
                .text("day_type", "1")
                .text("duty", "0")
                .text("only_save", "0");

            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&format!("{}{}", COOKIE_KEY, session_id))?);

            let res = self.client.post(url).headers(headers).multipart(form).send().await?;

            match res.status() {
                StatusCode::UNAUTHORIZED if retries < MAX_RETRY_COUNT => {
                    self.delete_session_id()?;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    retries += 1;
                    continue;
                }
                _ => return Ok(res.status()),
            }
        }
    }

    pub async fn login(&self, credentials: &LoginCredentials) -> Result<String, Box<dyn Error>> {
        let auth_url = format!("{}/{}", self.config.auth_url, AUTH_URL);
        let auth_res = self.client.post(auth_url).json(credentials).send().await?;
        let auth_body = auth_res.text().await?;
        let auth_session: AuthSession = serde_json::from_str(&auth_body)?;

        let login_url = format!("{}/{}", self.config.api_url, LOGIN_URL);
        let login_res = self
            .client
            .post(login_url)
            .header(header::AUTHORIZATION, format!("Bearer {}", auth_session.payload.token))
            .send()
            .await?;

        if let Some(cookie) = login_res.headers().get("Set-Cookie") {
            if let Ok(cookie_val) = cookie.to_str() {
                if let Some(portalsessid) = cookie_val.split(";").find(|c| c.starts_with(COOKIE_KEY)) {
                    let session_id = portalsessid.trim_start_matches(COOKIE_KEY);
                    return Ok(session_id.to_string());
                }
            }
        }

        Err("Login failed".into())
    }

    async fn get_session_id(&self) -> Result<String, Box<dyn Error>> {
        let session_id_file_path = DataStorage::new().get_path(SESSION_ID_FILE)?;
        let session_id_file_path_str = session_id_file_path.to_str().unwrap();
        if let Ok(session_id) = Self::read_session_id(&session_id_file_path_str) {
            Ok(session_id)
        } else {
            let password: String = Password::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter your password")
                .interact()
                .unwrap();
            let encoded_password = BASE64_STANDARD.encode(BASE64_STANDARD.encode(password));
            let login_credentials = LoginCredentials {
                login: self.config.login.to_string(),
                password: encoded_password,
            };
            let session_id = self.login(&login_credentials).await?;
            let _ = Self::write_session_id(&session_id_file_path_str, &session_id);
            Ok(session_id)
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

    pub async fn rest_dates(&self, year: NaiveDate) -> Result<HashSet<NaiveDate>, Box<dyn Error>> {
        let mut retries = 0;
        loop {
            let session_id = self.get_session_id().await?;
            let url = format!("{}/{}", self.config.api_url, REST_DATES_URL);
            let form = multipart::Form::new().text("year", year.format("%Y").to_string());
            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&format!("{}{}", COOKIE_KEY, session_id))?);

            let res = self.client.post(url).headers(headers).multipart(form).send().await?;

            match res.status() {
                StatusCode::UNAUTHORIZED if retries < MAX_RETRY_COUNT => {
                    self.delete_session_id()?;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    retries += 1;
                    continue;
                }
                _ => {
                    let rest_dates_response = res.json::<RestDatesResponse>().await?;
                    return Ok(rest_dates_response.unique_dates()?);
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SiConfig {
    pub login: String,
    pub auth_url: String,
    pub api_url: String,
}

impl SiConfig {
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "si".to_string(),
            name: "SiServer".to_string(),
        }
    }
    pub fn init(config: &Option<SiConfig>) -> Result<Self, Box<dyn Error>> {
        let config = config
            .clone()
            .or(Some(Self {
                login: "".to_string(),
                auth_url: "".to_string(),
                api_url: "".to_string(),
            }))
            .unwrap();
        println!("SiServer settings");
        Ok(Self {
            login: Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter your SiServer login")
                .default(config.login)
                .interact_text()?,
            auth_url: Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter your SiServer login URL")
                .default(config.auth_url)
                .interact_text()?,
            api_url: Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter the SiServer API URL")
                .default(config.api_url)
                .interact_text()?,
        })
    }
}
