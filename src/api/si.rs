//! Internal SiServer API client for company reporting and calendar integration.
//!
//! Provides integration with an internal company API system that handles employee
//! time tracking reports and company calendar information.
//!
//! ## Features
//!
//! - **Report Submission**: Submit daily and monthly time tracking reports
//! - **Calendar Integration**: Fetch company rest dates and holidays
//! - **Two-Stage Authentication**: LDAP authentication followed by session token exchange
//! - **Error Resilience**: Graceful handling of network failures and API errors
//! - **Session Management**: Automatic session caching and renewal
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::api::{Si, SiConfig};
//! use chrono::Local;
//!
//! let config = SiConfig {
//!     login: "username".to_string(),
//!     auth_url: "https://auth.company.com".to_string(),
//!     api_url: "https://api.company.com".to_string(),
//! };
//!
//! let mut si = Si::new(&config);
//! let today = Local::now().date_naive();
//! let rest_dates = si.rest_dates(today).await?;
//! ```

use crate::{
    api::Session,
    libs::{config::ConfigModule, messages::Message, secret::Secret},
    msg_error, msg_print,
};
use anyhow::Result;
use base64::prelude::*;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use dialoguer::{theme::ColorfulTheme, Input};
use reqwest::{
    header::{self, HeaderMap, HeaderValue, COOKIE},
    multipart, Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Maximum number of authentication retries before giving up.
/// SiServer has more complex auth flow, so we use the same conservative limit.
const MAX_RETRY_COUNT: i32 = 3;

/// Cookie name prefix used by SiServer for session identification.
const COOKIE_KEY: &str = "PORTALSESSID=";

/// Filename for storing SiServer session tokens in the user data directory.
const SESSION_ID_FILE: &str = ".si_session_id";

/// Filename for storing encrypted SiServer credentials for password caching.
const SECRET_FILE: &str = ".si_secret";

/// SiServer API endpoint for LDAP authentication (first stage).
const AUTH_URL: &str = "auth/ldap";

/// SiServer API endpoint for token-to-session exchange (second stage).
const LOGIN_URL: &str = "auth/login-by-token";

/// SiServer API endpoint for submitting daily time reports.
const REPORT_URL: &str = "report-card/send-daily-report";

/// SiServer API endpoint for submitting monthly summary reports.
const MONTHLY_REPORT_URL: &str = "report-card/send-monthly-report";

/// SiServer API endpoint for fetching company rest dates and holidays.
const REST_DATES_URL: &str = "report-card/get-rest-dates";

/// User credentials for SiServer authentication.
///
/// SiServer requires special password encoding (double base64) for security.
/// Credentials are only held in memory during the authentication process.
#[derive(Serialize, Clone, Debug)]
pub struct LoginCredentials {
    /// Username for LDAP authentication
    login: String,
    /// Double base64-encoded password for enhanced security
    password: String,
}

/// Response structure for SiServer LDAP authentication.
///
/// The first stage of authentication returns a temporary token that must
/// be exchanged for a session cookie in the second stage.
#[derive(Deserialize)]
pub struct AuthSession {
    /// Payload containing the authentication token
    payload: AuthPayload,
}

/// Authentication payload containing the temporary token.
///
/// This token is used to authenticate the second stage of the login process
/// where it's exchanged for a session cookie.
#[derive(Deserialize)]
pub struct AuthPayload {
    /// Temporary authentication token for session exchange
    token: String,
}

/// Response structure for SiServer rest dates API.
///
/// SiServer provides company calendar information including various types
/// of non-working days such as holidays, vacation days, and weekend days.
/// Different date arrays represent different types of rest periods.
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
    /// Parses and combines all rest dates into a single unified set.
    ///
    /// This method processes all three categories of rest dates and combines them
    /// into a single `HashSet` for easy lookup operations. Duplicate dates across
    /// categories are automatically deduplicated.
    ///
    /// ## Date Format Handling
    ///
    /// The API returns dates in "YYYY-MM-DD" format. Invalid date strings are
    /// silently ignored to handle potential API inconsistencies gracefully.
    ///
    /// # Returns
    ///
    /// * `Result<HashSet<NaiveDate>>` - Unified set of all rest dates
    ///
    /// # Errors
    ///
    /// Currently cannot fail, but returns `Result` for future error handling
    /// such as date validation or API response verification.
    pub fn unique_dates(&self) -> Result<HashSet<NaiveDate>> {
        let mut date_set = HashSet::new();

        // Process all three date categories
        self.process_dates(&self.dates, &mut date_set)?;
        self.process_dates(&self.v_dates, &mut date_set)?;
        self.process_dates(&self.w_dates, &mut date_set)?;

        Ok(date_set)
    }

    /// Helper function to parse date strings and add them to the result set.
    ///
    /// Processes a vector of date strings, attempting to parse each one into
    /// a `NaiveDate`. Invalid dates are silently skipped to handle API
    /// inconsistencies without failing the entire operation.
    ///
    /// # Arguments
    ///
    /// * `dates` - Vector of date strings in "YYYY-MM-DD" format
    /// * `date_set` - Mutable reference to the result set for adding parsed dates
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` as this operation cannot fail.
    fn process_dates(&self, dates: &Vec<String>, date_set: &mut HashSet<NaiveDate>) -> Result<()> {
        dates
            .iter()
            .filter_map(|date_str| NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok())
            .for_each(|date| {
                date_set.insert(date);
            });
        Ok(())
    }
}

/// SiServer API client with advanced session management.
///
/// This client handles the complex two-stage authentication flow required by
/// SiServer and provides methods for report submission and calendar data retrieval.
/// It implements resilient error handling to ensure application stability.
///
/// ## Thread Safety
///
/// The client is not thread-safe due to mutable retry state. Each thread
/// should use its own client instance for concurrent operations.
///
/// ## Authentication Architecture
///
/// SiServer uses a sophisticated authentication system:
/// 1. **LDAP Stage**: Credentials sent to LDAP endpoint, token received
/// 2. **Session Stage**: Token sent to session endpoint, cookie received
/// 3. **API Usage**: Cookie included in all subsequent API requests
///
/// This design provides enhanced security but requires careful session management.
#[derive(Debug)]
pub struct Si {
    /// HTTP client for making API requests with connection pooling
    client: Client,
    /// Configuration containing API endpoints and user information
    config: SiConfig,
    /// In-memory storage for authentication credentials during auth process
    credentials: Option<LoginCredentials>,
    /// Counter for tracking authentication retry attempts
    retries: i32,
}

impl Session for Si {
    /// Performs two-stage authentication with SiServer.
    ///
    /// This method implements SiServer's unique authentication flow which requires
    /// two separate API calls to establish a session. The process is more complex
    /// than standard session authentication but provides enhanced security.
    ///
    /// ## Authentication Process
    ///
    /// 1. **LDAP Authentication**: Send credentials to LDAP endpoint
    /// 2. **Token Extraction**: Parse authentication token from response
    /// 3. **Session Exchange**: Send token to session endpoint with Bearer auth
    /// 4. **Cookie Extraction**: Parse session cookie from Set-Cookie header
    /// 5. **Format Preparation**: Extract session ID for use in subsequent requests
    ///
    /// ## Error Scenarios
    ///
    /// - LDAP authentication failure (invalid credentials)
    /// - Token parsing failure (unexpected response format)
    /// - Session exchange failure (token expired or invalid)
    /// - Cookie extraction failure (missing or malformed Set-Cookie header)
    ///
    /// # Returns
    ///
    /// Returns the session ID string extracted from the PORTALSESSID cookie.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No credentials have been set (programming error)
    /// - Network requests fail
    /// - LDAP authentication fails
    /// - Token or cookie parsing fails
    /// - Authentication flow completes but no valid session is established
    async fn login(&self) -> Result<String> {
        // Ensure credentials are available for authentication
        let credentials = self.credentials.clone().expect("Credentials not set!");

        // Stage 1: LDAP Authentication
        let auth_url = format!("{}/{}", self.config.auth_url, AUTH_URL);
        let auth_res = self.client.post(auth_url).json(&credentials).send().await?;
        let auth_body = auth_res.text().await?;
        let auth_session: AuthSession = serde_json::from_str(&auth_body)?;

        // Stage 2: Token-to-Session Exchange
        let login_url = format!("{}/{}", self.config.api_url, LOGIN_URL);
        let login_res = self
            .client
            .post(login_url)
            .header(header::AUTHORIZATION, format!("Bearer {}", auth_session.payload.token))
            .send()
            .await?;

        // Stage 3: Cookie Extraction
        if let Some(cookie) = login_res.headers().get("Set-Cookie") {
            if let Ok(cookie_val) = cookie.to_str() {
                // Find the PORTALSESSID cookie in the Set-Cookie header
                if let Some(portalsessid) = cookie_val.split(";").find(|c| c.starts_with(COOKIE_KEY)) {
                    let session_id = portalsessid.trim_start_matches(COOKIE_KEY);
                    return Ok(session_id.to_string());
                }
            }
        }

        // Authentication completed but no valid session cookie was found
        anyhow::bail!("Login failed")
    }

    /// Sets user credentials with SiServer-specific password encoding.
    ///
    /// SiServer requires passwords to be double base64-encoded for security.
    /// This method handles the encoding and stores credentials in memory for
    /// use during the authentication process.
    ///
    /// ## Password Encoding
    ///
    /// The password undergoes double base64 encoding:
    /// 1. First encoding: `base64(password)`
    /// 2. Second encoding: `base64(base64(password))`
    ///
    /// This provides additional security layers for credential transmission.
    ///
    /// # Arguments
    ///
    /// * `password` - The user's SiServer password in plain text
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` as encoding cannot fail.
    fn set_credentials(&mut self, password: &str) -> Result<()> {
        // Apply double base64 encoding as required by SiServer
        let encoded_password = BASE64_STANDARD.encode(BASE64_STANDARD.encode(password));

        self.credentials = Some(LoginCredentials {
            login: self.config.login.to_string(),
            password: encoded_password,
        });
        Ok(())
    }

    /// Returns the filename for storing SiServer session tokens.
    ///
    /// The session file is stored in the user's application data directory
    /// and contains the cached session token for automatic login restoration.
    fn session_id_file(&self) -> &str {
        SESSION_ID_FILE
    }

    /// Returns a configured Secret instance for secure password prompting.
    ///
    /// The Secret manager handles secure password input with hidden characters
    /// and optional encrypted caching in the user's data directory.
    ///
    /// # Returns
    ///
    /// A configured `Secret` instance with SiServer-specific prompts and file names.
    fn secret(&self) -> Secret {
        Secret::new(SECRET_FILE, "Enter your SiServer password")
    }

    /// Returns the current authentication retry count.
    ///
    /// Used by the session management system to track failed authentication
    /// attempts and implement retry limits.
    fn retry(&self) -> i32 {
        self.retries
    }

    /// Increments the authentication retry counter.
    ///
    /// Called after each failed authentication attempt to track progress
    /// toward the maximum retry limit.
    fn inc_retry(&mut self) {
        self.retries += 1;
    }
}

impl Si {
    /// Creates a new SiServer API client instance.
    ///
    /// Initializes the HTTP client with default settings suitable for SiServer API
    /// interactions. The client is configured for both JSON and multipart requests
    /// to handle different SiServer endpoints appropriately.
    ///
    /// # Arguments
    ///
    /// * `config` - SiServer configuration containing API endpoints and user information
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::api::{Si, SiConfig};
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

    /// Submits a daily time tracking report to SiServer.
    ///
    /// This method sends formatted daily report data to the SiServer API for
    /// payroll and time tracking integration. It handles session management
    /// and implements retry logic for authentication failures.
    ///
    /// ## Report Format
    ///
    /// The report data should be a JSON string containing:
    /// - Work hours and break information
    /// - Task completion details
    /// - Productivity metrics
    /// - Any relevant metadata for the specified date
    ///
    /// ## Session Management
    ///
    /// The method implements automatic session handling:
    /// 1. **Session Retrieval**: Get or create a valid session token
    /// 2. **Report Submission**: Send report data with session authentication
    /// 3. **Error Handling**: Detect expired sessions and retry with re-authentication
    /// 4. **Status Return**: Return HTTP status for caller handling
    ///
    /// # Arguments
    ///
    /// * `data` - JSON string containing the formatted report data
    /// * `date` - The date for which the report is being submitted
    ///
    /// # Returns
    ///
    /// Returns the HTTP status code from the API response, allowing callers
    /// to determine success or specific failure modes.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Session management fails persistently
    /// - Network request fails
    /// - Request formatting fails
    /// - Duration conversion fails (internal error)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use kasl::api::{Si, SiConfig};
    /// # use chrono::Local;
    /// # use anyhow::Result;
    /// # async fn example() -> Result<()> {
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
    pub async fn send(&mut self, data: &String, date: &NaiveDate) -> Result<StatusCode> {
        loop {
            // Get valid session for API request
            let session_id = self.get_session_id().await?;
            let url = format!("{}/{}", self.config.api_url, REPORT_URL);
            let date = date.format("%Y-%m-%d").to_string();

            // Prepare multipart form data for submission
            let form = multipart::Form::new()
                .text("date", date)
                .text("tasks", data.clone())
                .text("comment", "")
                .text("day_type", "1")
                .text("duty", "0")
                .text("only_save", "0");

            // Set up authentication headers
            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&format!("{}{}", COOKIE_KEY, session_id))?);

            // Submit the report
            let res = match self.client.post(url).headers(headers).multipart(form).send().await {
                Ok(response) => response,
                Err(_) => return Ok(StatusCode::BAD_REQUEST), // Network error fallback
            };

            // Handle response and potential session expiration
            match res.status() {
                StatusCode::UNAUTHORIZED if self.retries < MAX_RETRY_COUNT => {
                    // Session expired - clear cache and retry
                    self.delete_session_id()?;
                    tokio::time::sleep(Duration::seconds(1).to_std()?).await;
                    self.retries += 1;
                    continue;
                }
                _ => return Ok(res.status()),
            }
        }
    }

    /// Submits a monthly summary report to SiServer.
    ///
    /// Sends aggregated monthly statistics to the SiServer API for organizational
    /// reporting and payroll integration. The report covers the entire month
    /// containing the specified date.
    ///
    /// ## Monthly Report Contents
    ///
    /// The system automatically generates a summary containing:
    /// - Total working hours for the month
    /// - Number of working days
    /// - Average daily productivity
    /// - Compliance with company policies
    ///
    /// ## Last Working Day Logic
    ///
    /// Monthly reports are typically submitted on the last working day of each month.
    /// The system can automatically detect this condition and prompt for submission.
    ///
    /// # Arguments
    ///
    /// * `date` - Any date within the target month for report generation
    ///
    /// # Returns
    ///
    /// Returns the HTTP status code from the API response.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use kasl::api::{Si, SiConfig};
    /// # use chrono::Local;
    /// # use anyhow::Result;
    /// # async fn example() -> Result<()> {
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
        loop {
            // Get valid session for API request
            let session_id = self.get_session_id().await?;
            let url = format!("{}/{}", self.config.api_url, MONTHLY_REPORT_URL);
            let (year, month) = (date.year(), date.month());

            // Prepare monthly report form data
            let form = multipart::Form::new().text("month", month.to_string()).text("year", year.to_string());

            // Set up authentication headers
            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&format!("{}{}", COOKIE_KEY, session_id))?);

            // Submit the monthly report
            let res = match self.client.post(url).headers(headers).multipart(form).send().await {
                Ok(response) => response,
                Err(_) => return Ok(StatusCode::BAD_REQUEST), // Network error fallback
            };

            // Handle response and potential session expiration
            match res.status() {
                StatusCode::UNAUTHORIZED if self.retries < MAX_RETRY_COUNT => {
                    // Session expired - clear cache and retry
                    self.delete_session_id()?;
                    tokio::time::sleep(Duration::seconds(1).to_std()?).await;
                    self.retries += 1;
                    continue;
                }
                _ => return Ok(res.status()),
            }
        }
    }

    /// Fetches company rest dates and holidays for the specified year.
    ///
    /// This method retrieves the official company calendar including holidays,
    /// vacation days, and extended weekend periods. The data is used for accurate
    /// productivity calculations and report generation.
    ///
    /// ## Error Resilience
    ///
    /// This function prioritizes application stability over data completeness:
    /// - Network errors return empty results rather than failing
    /// - Authentication failures are logged but don't interrupt operation
    /// - API parsing errors result in empty calendar (graceful degradation)
    /// - Session failures are handled with automatic retry
    ///
    /// This design ensures that calendar integration enhances functionality
    /// without breaking core time tracking features when services are unavailable.
    ///
    /// ## Date Processing
    ///
    /// The API returns three categories of rest dates:
    /// - Regular holidays (national and company holidays)
    /// - Vacation dates (company-specific rest periods)
    /// - Weekend extensions (long weekend periods)
    ///
    /// All categories are combined into a single set for unified processing.
    ///
    /// # Arguments
    ///
    /// * `year` - Any date within the target year for calendar retrieval
    ///
    /// # Returns
    ///
    /// Returns a `HashSet<NaiveDate>` containing all rest dates for the year.
    /// Returns an empty set on any error to ensure graceful degradation.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use kasl::api::{Si, SiConfig};
    /// # use chrono::Local;
    /// # use anyhow::Result;
    /// # async fn example() -> Result<()> {
    /// let mut si = Si::new(&config);
    /// let this_year = Local::now().date_naive();
    ///
    /// let rest_dates = si.rest_dates(this_year).await?;
    /// println!("Found {} rest dates this year", rest_dates.len());
    ///
    /// // Check if a specific date is a rest day
    /// let today = Local::now().date_naive();
    /// if rest_dates.contains(&today) {
    ///     println!("Today is a company rest day");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn rest_dates(&mut self, year: NaiveDate) -> Result<HashSet<NaiveDate>> {
        loop {
            // Get valid session for API request
            let session_id = match self.get_session_id().await {
                Ok(id) => id,
                Err(e) => {
                    msg_error!(Message::SiServerSessionFailed(e.to_string()));
                    return Ok(HashSet::new()); // Return empty set on session failure
                }
            };

            // Prepare rest dates request
            let url = format!("{}/{}", self.config.api_url, REST_DATES_URL);
            let form = multipart::Form::new().text("year", year.format("%Y").to_string());
            let mut headers = HeaderMap::new();
            headers.insert(COOKIE, HeaderValue::from_str(&format!("{}{}", COOKIE_KEY, session_id))?);

            // Request rest dates from API
            let res = match self.client.post(url).headers(headers).multipart(form).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    msg_error!(Message::SiServerRestDatesFailed(e.to_string()));
                    return Ok(HashSet::new()); // Return empty set on network error
                }
            };

            // Handle response and potential session expiration
            match res.status() {
                StatusCode::UNAUTHORIZED if self.retries < MAX_RETRY_COUNT => {
                    // Session expired - clear cache and retry
                    self.delete_session_id()?;
                    self.retries += 1;
                    continue;
                }
                _ => {
                    // Process successful response or non-recoverable error
                    return match res.json::<RestDatesResponse>().await {
                        Ok(response) => Ok(response.unique_dates()?),
                        Err(e) => {
                            msg_error!(Message::SiServerRestDatesParsingFailed(e.to_string()));
                            Ok(HashSet::new()) // Return empty set on parsing error
                        }
                    };
                }
            }
        }
    }

    /// Determines if the specified date is the last working day of its month.
    ///
    /// This utility function calculates whether a given date represents the final
    /// working day in its month, which is useful for triggering monthly report
    /// submissions and other end-of-month processing.
    ///
    /// ## Algorithm
    ///
    /// The calculation process:
    /// 1. **Find Month End**: Determine the last calendar day of the month
    /// 2. **Weekend Adjustment**: Move backward from weekends to find working days
    /// 3. **Comparison**: Check if the input date matches the calculated last working day
    ///
    /// ## Limitations
    ///
    /// Currently only considers weekends (Saturday/Sunday) as non-working days.
    /// Future versions may integrate with the rest dates API to consider holidays
    /// and company-specific non-working days for more accurate calculations.
    ///
    /// # Arguments
    ///
    /// * `date` - The date to check against the last working day
    ///
    /// # Returns
    ///
    /// Returns `true` if the date is the last working day of its month,
    /// `false` otherwise.
    ///
    /// # Errors
    ///
    /// Currently cannot fail, but returns `Result` for consistency and
    /// future enhancement with holiday integration.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use kasl::api::{Si, SiConfig};
    /// # use chrono::NaiveDate;
    /// # use anyhow::Result;
    /// # fn example() -> Result<()> {
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

        // Calculate the last day of the current month
        let mut last_day_of_month = NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap().pred_opt().unwrap();

        // Move backward from weekends to find the last working day
        while matches!(last_day_of_month.weekday(), Weekday::Sat | Weekday::Sun) {
            last_day_of_month = last_day_of_month - Duration::days(1);
        }

        // Check if the input date matches the calculated last working day
        Ok(date == &last_day_of_month)
    }
}

/// Configuration for SiServer API integration.
///
/// This structure holds the necessary information for connecting to internal
/// SiServer systems. Unlike other API integrations, SiServer requires separate
/// authentication and API endpoints due to its sophisticated security architecture.
///
/// ## Multi-Endpoint Architecture
///
/// SiServer uses different endpoints for different purposes:
/// - **Authentication URL**: LDAP authentication endpoint
/// - **API URL**: Main API endpoint for reports and data
/// - **Separation Benefits**: Enhanced security, load distribution, service isolation
///
/// ## Security Considerations
///
/// - Passwords are never stored in configuration files
/// - Only username and endpoints are persisted
/// - Session tokens are cached separately with encryption
/// - Double base64 password encoding for transmission security
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SiConfig {
    /// Username for SiServer authentication.
    ///
    /// This should be the corporate username used for LDAP authentication.
    /// Typically matches the username used for other company systems.
    pub login: String,

    /// URL for the SiServer authentication endpoint.
    ///
    /// This endpoint handles LDAP authentication and token generation.
    /// Example: `https://auth.company.com`
    ///
    /// This is separate from the main API URL due to SiServer's security architecture.
    pub auth_url: String,

    /// Base URL for the main SiServer API endpoints.
    ///
    /// This endpoint handles report submission and data retrieval operations.
    /// Example: `https://api.company.com`
    ///
    /// All API operations (reports, calendar) use this base URL.
    pub api_url: String,
}

impl SiConfig {
    /// Returns the configuration module metadata for SiServer.
    ///
    /// Used by the configuration system to identify and manage
    /// SiServer-specific settings during interactive setup.
    ///
    /// # Returns
    ///
    /// A `ConfigModule` with SiServer identification information.
    pub fn module() -> ConfigModule {
        ConfigModule {
            key: "si".to_string(),
            name: "SiServer".to_string(),
        }
    }

    /// Runs an interactive configuration setup for SiServer integration.
    ///
    /// Prompts the user for SiServer connection details including username
    /// and both authentication and API endpoints. Uses existing configuration
    /// values as defaults if available.
    ///
    /// ## Interactive Prompts
    ///
    /// 1. **Username**: Corporate username for LDAP authentication
    /// 2. **Authentication URL**: LDAP endpoint for token generation
    /// 3. **API URL**: Main API endpoint for reports and data operations
    ///
    /// All prompts show existing values as defaults if configuration already
    /// exists, making it easy to update specific values without re-entering everything.
    ///
    /// ## Configuration Validation
    ///
    /// While this method doesn't validate actual connectivity, it provides
    /// helpful prompts to guide users toward correct configuration values
    /// for their corporate SiServer deployment.
    ///
    /// # Arguments
    ///
    /// * `config` - Existing SiServer configuration to use as defaults (if any)
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - New SiServer configuration with user input
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Terminal input/output fails
    /// - User cancels the configuration process
    /// - Input validation fails
    ///
    /// # Example
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
        // Use existing configuration as defaults, or create empty defaults
        let config = config
            .clone()
            .or(Some(Self {
                login: "".to_string(),
                auth_url: "".to_string(),
                api_url: "".to_string(),
            }))
            .unwrap();

        // Display configuration module header
        msg_print!(Message::ConfigModuleSiServer);

        // Interactive configuration with existing values as defaults
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
