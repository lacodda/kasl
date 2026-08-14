//! SiServer client: daily/monthly report submission and the company
//! rest-date calendar.
//!
//! ```rust,no_run
//! # use kasl::api::si::{Si, SiConfig};
//! # use chrono::Local;
//! # async fn f() -> anyhow::Result<()> {
//! let config = SiConfig {
//!     login: "username".to_string(),
//!     auth_url: "https://auth.company.com".to_string(),
//!     api_url: "https://api.company.com".to_string(),
//! };
//!
//! let mut si = Si::new(&config);
//! let today = Local::now().date_naive();
//! let rest_dates = si.rest_dates(today).await?;
//! # Ok(())
//! # }
//! ```

use crate::{
    api::Session,
    libs::{config::ConfigModule, messages::Message, secret::Secret},
    msg_error, msg_print,
};
use anyhow::Result;
use base64::prelude::*;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use dialoguer::{Input, theme::ColorfulTheme};
use reqwest::{
    Client, StatusCode,
    header::{self, COOKIE, HeaderMap, HeaderValue},
    multipart,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MAX_RETRY_COUNT: i32 = 3;
const COOKIE_KEY: &str = "PORTALSESSID=";
const SESSION_ID_FILE: &str = ".si_session_id";
const SECRET_FILE: &str = ".si_secret";
const AUTH_URL: &str = "auth/ldap";
const LOGIN_URL: &str = "auth/login-by-token";
const REPORT_URL: &str = "report-card/send-daily-report";
const MONTHLY_REPORT_URL: &str = "report-card/send-monthly-report";
const REST_DATES_URL: &str = "report-card/get-rest-dates";

/// Login and the double-base64-encoded password SiServer expects.
#[derive(Serialize, Clone, Debug)]
pub struct LoginCredentials {
    login: String,
    password: String,
}

/// First-stage (LDAP) response carrying the temporary token.
#[derive(Deserialize)]
pub struct AuthSession {
    payload: AuthPayload,
}

#[derive(Deserialize)]
pub struct AuthPayload {
    token: String,
}

/// Rest-date calendar response: three categories of non-working days.
#[derive(Debug, Deserialize)]
pub struct RestDatesResponse {
    /// Regular rest dates (general holidays)
    dates: Vec<String>,
    /// Vacation dates (company-specific holidays)
    v_dates: Vec<String>,
    /// Weekend dates (extended weekend periods)
    w_dates: Vec<String>,
}

impl RestDatesResponse {
    /// Merges all three categories into one deduplicated set; date strings
    /// that fail to parse are skipped rather than failing the calendar.
    pub fn unique_dates(&self) -> Result<HashSet<NaiveDate>> {
        let mut date_set = HashSet::new();

        self.process_dates(&self.dates, &mut date_set)?;
        self.process_dates(&self.v_dates, &mut date_set)?;
        self.process_dates(&self.w_dates, &mut date_set)?;

        Ok(date_set)
    }

    fn process_dates(&self, dates: &[String], date_set: &mut HashSet<NaiveDate>) -> Result<()> {
        dates
            .iter()
            .filter_map(|date_str| NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok())
            .for_each(|date| {
                date_set.insert(date);
            });
        Ok(())
    }
}

/// SiServer client. Authentication is two-stage: LDAP login yields a
/// token, the token is exchanged for a `PORTALSESSID` cookie, and the
/// cookie rides on every API call.
#[derive(Debug)]
pub struct Si {
    client: Client,
    config: SiConfig,
    /// Held in memory only while authenticating.
    credentials: Option<LoginCredentials>,
    retries: i32,
}

impl Session for Si {
    /// Runs the two-stage login and returns the session id extracted from
    /// the `Set-Cookie` header.
    async fn login(&self) -> Result<String> {
        let credentials = self.credentials.clone().expect("Credentials not set!");

        // Stage 1: LDAP authentication yields a bearer token.
        let auth_url = format!("{}/{}", self.config.auth_url, AUTH_URL);
        let auth_res = self.client.post(auth_url).json(&credentials).send().await?;
        let auth_body = auth_res.text().await?;
        let auth_session: AuthSession = serde_json::from_str(&auth_body)?;

        // Stage 2: the token buys a session cookie.
        let login_url = format!("{}/{}", self.config.api_url, LOGIN_URL);
        let login_res = self
            .client
            .post(login_url)
            .header(header::AUTHORIZATION, format!("Bearer {}", auth_session.payload.token))
            .send()
            .await?;

        if let Some(cookie) = login_res.headers().get("Set-Cookie")
            && let Ok(cookie_val) = cookie.to_str()
            && let Some(portalsessid) = cookie_val.split(";").find(|c| c.starts_with(COOKIE_KEY))
        {
            let session_id = portalsessid.trim_start_matches(COOKIE_KEY);
            return Ok(session_id.to_string());
        }

        anyhow::bail!("Login failed")
    }

    /// Stores the credentials, double-base64-encoding the password as
    /// SiServer requires.
    fn set_credentials(&mut self, password: &str) -> Result<()> {
        let encoded_password = BASE64_STANDARD.encode(BASE64_STANDARD.encode(password));

        self.credentials = Some(LoginCredentials {
            login: self.config.login.to_string(),
            password: encoded_password,
        });
        Ok(())
    }

    fn session_id_file(&self) -> &str {
        SESSION_ID_FILE
    }

    fn secret(&self) -> Secret {
        Secret::new(SECRET_FILE, "Enter your SiServer password")
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

impl Si {
    /// Builds a client from the config; no network activity yet.
    ///
    /// ```rust,no_run
    /// use kasl::api::si::{Si, SiConfig};
    ///
    /// let config = SiConfig {
    ///     login: "username".to_string(),
    ///     auth_url: "https://auth.company.com".to_string(),
    ///     api_url: "https://api.company.com".to_string(),
    /// };
    /// let si = Si::new(&config);
    /// ```
    pub fn new(config: &SiConfig) -> Self {
        Self {
            client: Client::new(),
            config: config.clone(),
            credentials: None,
            retries: 0,
        }
    }

    /// Submits the daily report (tasks as JSON) for the date. On a 401 the
    /// cached session is dropped and the call retried up to the limit; a
    /// network error maps to `BAD_REQUEST` so a scheduled send fails soft.
    ///
    /// ```rust,no_run
    /// # use kasl::api::si::{Si, SiConfig};
    /// # use chrono::Local;
    /// # use anyhow::Result;
    /// # async fn example() -> Result<()> {
    /// # let config = SiConfig {
    /// #     login: "username".to_string(),
    /// #     auth_url: "https://auth.company.com".to_string(),
    /// #     api_url: "https://api.company.com".to_string(),
    /// # };
    /// let mut si = Si::new(&config);
    /// let report_data = r#"{"hours": 8, "tasks": 5}"#.to_string();
    /// let today = Local::now().date_naive();
    ///
    /// let status = si.send(&report_data, &today).await?;
    /// if status.is_success() {
    ///     println!("Report submitted successfully");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send(&mut self, data: &str, date: &NaiveDate) -> Result<StatusCode> {
        let mut local_retries = 0;
        loop {
            let session_id = self.get_session_id().await?;
            let url = format!("{}/{}", self.config.api_url, REPORT_URL);
            let date = date.format("%Y-%m-%d").to_string();

            let form = multipart::Form::new()
                .text("date", date)
                .text("tasks", data.to_owned())
                .text("comment", "")
                .text("day_type", "1")
                .text("duty", "0")
                .text("only_save", "0");

            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&format!("{}{}", COOKIE_KEY, session_id))?);

            let res = match self.client.post(url).headers(headers).multipart(form).send().await {
                Ok(response) => response,
                Err(_) => return Ok(StatusCode::BAD_REQUEST), // Network error fallback
            };

            match res.status() {
                StatusCode::UNAUTHORIZED if local_retries < MAX_RETRY_COUNT => {
                    // Session expired - clear cache and retry
                    self.delete_session_id()?;
                    tokio::time::sleep(Duration::seconds(1).to_std()?).await;
                    local_retries += 1;
                    continue;
                }
                _ => return Ok(res.status()),
            }
        }
    }

    /// Submits the monthly report for the month containing `date`, with
    /// the same 401-retry and network-error behavior as [`Si::send`].
    ///
    /// ```rust,no_run
    /// # use kasl::api::si::{Si, SiConfig};
    /// # use chrono::Local;
    /// # use anyhow::Result;
    /// # async fn example() -> Result<()> {
    /// # let config = SiConfig {
    /// #     login: "username".to_string(),
    /// #     auth_url: "https://auth.company.com".to_string(),
    /// #     api_url: "https://api.company.com".to_string(),
    /// # };
    /// let mut si = Si::new(&config);
    /// let today = Local::now().date_naive();
    ///
    /// if si.is_last_working_day_of_month(&today)? {
    ///     let status = si.send_monthly(&today).await?;
    ///     if status.is_success() {
    ///         println!("Monthly report submitted");
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_monthly(&mut self, date: &NaiveDate) -> Result<StatusCode> {
        let mut local_retries = 0;
        loop {
            let session_id = self.get_session_id().await?;
            let url = format!("{}/{}", self.config.api_url, MONTHLY_REPORT_URL);
            let (year, month) = (date.year(), date.month());

            let form = multipart::Form::new().text("month", month.to_string()).text("year", year.to_string());

            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&format!("{}{}", COOKIE_KEY, session_id))?);

            let res = match self.client.post(url).headers(headers).multipart(form).send().await {
                Ok(response) => response,
                Err(_) => return Ok(StatusCode::BAD_REQUEST), // Network error fallback
            };

            match res.status() {
                StatusCode::UNAUTHORIZED if local_retries < MAX_RETRY_COUNT => {
                    self.delete_session_id()?;
                    tokio::time::sleep(Duration::seconds(1).to_std()?).await;
                    local_retries += 1;
                    continue;
                }
                _ => return Ok(res.status()),
            }
        }
    }

    /// Fetches the company rest-date calendar for the year of `date`.
    ///
    /// Every failure path - session, network, parsing - logs and returns
    /// an empty set: the calendar makes reports nicer, and its absence
    /// must not break them.
    ///
    /// ```rust,no_run
    /// # use kasl::api::si::{Si, SiConfig};
    /// # use chrono::Local;
    /// # use anyhow::Result;
    /// # async fn example() -> Result<()> {
    /// # let config = SiConfig {
    /// #     login: "username".to_string(),
    /// #     auth_url: "https://auth.company.com".to_string(),
    /// #     api_url: "https://api.company.com".to_string(),
    /// # };
    /// let mut si = Si::new(&config);
    /// let this_year = Local::now().date_naive();
    ///
    /// let rest_dates = si.rest_dates(this_year).await?;
    /// println!("Found {} rest dates this year", rest_dates.len());
    ///
    /// let today = Local::now().date_naive();
    /// if rest_dates.contains(&today) {
    ///     println!("Today is a company rest day");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn rest_dates(&mut self, year: NaiveDate) -> Result<HashSet<NaiveDate>> {
        let mut local_retries = 0;
        loop {
            let session_id = match self.get_session_id().await {
                Ok(id) => id,
                Err(e) => {
                    msg_error!(Message::SiServerSessionFailed(e.to_string()));
                    return Ok(HashSet::new());
                }
            };

            let url = format!("{}/{}", self.config.api_url, REST_DATES_URL);
            let form = multipart::Form::new().text("year", year.format("%Y").to_string());
            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&format!("{}{}", COOKIE_KEY, session_id))?);

            let res = match self.client.post(url).headers(headers).multipart(form).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    msg_error!(Message::SiServerRestDatesFailed(e.to_string()));
                    return Ok(HashSet::new());
                }
            };

            match res.status() {
                StatusCode::UNAUTHORIZED if local_retries < MAX_RETRY_COUNT => {
                    self.delete_session_id()?;
                    local_retries += 1;
                    continue;
                }
                _ => {
                    return match res.json::<RestDatesResponse>().await {
                        Ok(response) => Ok(response.unique_dates()?),
                        Err(e) => {
                            msg_error!(Message::SiServerRestDatesParsingFailed(e.to_string()));
                            Ok(HashSet::new())
                        }
                    };
                }
            }
        }
    }

    /// True when `date` is the month's last working day (weekends walked
    /// back; company holidays are not consulted).
    ///
    /// ```rust,no_run
    /// # use kasl::api::si::{Si, SiConfig};
    /// # use chrono::NaiveDate;
    /// # use anyhow::Result;
    /// # fn example() -> Result<()> {
    /// # let config = SiConfig {
    /// #     login: "username".to_string(),
    /// #     auth_url: "https://auth.company.com".to_string(),
    /// #     api_url: "https://api.company.com".to_string(),
    /// # };
    /// let si = Si::new(&config);
    /// let date = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(); // January 31st
    ///
    /// if si.is_last_working_day_of_month(&date)? {
    ///     println!("Time to submit monthly report!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_last_working_day_of_month(&self, date: &NaiveDate) -> Result<bool> {
        let (year, month) = (date.year(), date.month());

        let mut last_day_of_month = NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap().pred_opt().unwrap();

        while matches!(last_day_of_month.weekday(), Weekday::Sat | Weekday::Sun) {
            last_day_of_month -= Duration::days(1);
        }

        Ok(date == &last_day_of_month)
    }
}

/// SiServer connection settings; auth and API live on separate hosts.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SiConfig {
    /// Corporate username for LDAP authentication.
    pub login: String,

    /// LDAP authentication endpoint.
    pub auth_url: String,

    /// Base URL for reports and calendar data.
    pub api_url: String,
}

impl SiConfig {
    /// Module metadata for the setup wizard.
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "si".to_string(),
            name: "SiServer".to_string(),
        }
    }

    /// Interactive setup; existing values become the prompt defaults.
    ///
    /// ```rust,no_run
    /// # use kasl::api::SiConfig;
    /// # use anyhow::Result;
    /// # fn example() -> Result<()> {
    /// let existing_config = Some(SiConfig {
    ///     login: "olduser".to_string(),
    ///     auth_url: "https://old-auth.com".to_string(),
    ///     api_url: "https://old-api.com".to_string(),
    /// });
    ///
    /// let new_config = SiConfig::init(&existing_config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn init(config: &Option<SiConfig>) -> Result<Self> {
        let config = config.clone().unwrap_or(Self {
            login: "".to_string(),
            auth_url: "".to_string(),
            api_url: "".to_string(),
        });

        msg_print!(Message::ConfigModuleSiServer);

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
