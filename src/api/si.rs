use crate::libs::config::Config;
use base64::prelude::*;
use chrono::prelude::Local;
use dialoguer::{theme::ColorfulTheme, Input};
use reqwest::{
    header::{self, HeaderMap, HeaderValue, COOKIE},
    multipart, Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs,
    io::{self, Write},
    time::Duration,
};

const COOKIE_KEY: &str = "PORTALSESSID=";
const SESSION_ID_FILE: &str = ".session_id";
const AUTH_URL: &str = "auth/ldap";
const LOGIN_URL: &str = "auth/login-by-token";
const REPORT_URL: &str = "report-card/get-rest-dates";
// const REPORT_URL: &str = "report-card/send-daily-report";

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

pub struct Si {
    client: Client,
    config: Config,
}

impl Si {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            config: config.clone(),
        }
    }

    pub async fn send(&self, data: String) -> Result<StatusCode, Box<dyn Error>> {
        let session_id = self.get_session_id().await?;
        let url = format!("{}/{}", self.config.si.api_url, REPORT_URL);
        let date = Local::now().format("%Y-%m-%d").to_string();
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

        if res.status() == 401 {
            Self::delete_session_id(SESSION_ID_FILE)?;
            tokio::time::sleep(Duration::from_secs(1)).await;
            return Box::pin(async move { self.send(data).await }).await;
        }

        Ok(res.status())
    }

    pub async fn login(&self, credentials: &LoginCredentials) -> Result<String, Box<dyn Error>> {
        let auth_url = format!("{}/{}", self.config.si.auth_url, AUTH_URL);
        let auth_res = self.client.post(auth_url).json(credentials).send().await?;
        let auth_body = auth_res.text().await?;
        let auth_session: AuthSession = serde_json::from_str(&auth_body)?;

        let login_url = format!("{}/{}", self.config.si.api_url, LOGIN_URL);
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

        println!("Token:\n{:#?}", auth_session.payload.token);
        Err("Login failed".into())
    }

    async fn get_session_id(&self) -> Result<String, Box<dyn Error>> {
        if let Ok(session_id) = Self::read_session_id(SESSION_ID_FILE) {
            Ok(session_id)
        } else {
            let password: String = Input::with_theme(&ColorfulTheme::default()).with_prompt("Enter your password").interact_text().unwrap();
            let encoded_password = BASE64_STANDARD.encode(BASE64_STANDARD.encode(password));
            println!("{}", &encoded_password);
            let login_credentials = LoginCredentials {
                login: self.config.si.login.to_string(),
                password: encoded_password,
            };
            let session_id = self.login(&login_credentials).await?;
            let _ = Self::write_session_id(SESSION_ID_FILE, &session_id);
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

    fn delete_session_id(file_name: &str) -> io::Result<()> {
        fs::remove_file(file_name)
    }
}
