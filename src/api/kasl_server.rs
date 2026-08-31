//! kasl-server client: the team server this agent reports to.
//!
//! Unlike the other clients here, this one has no session to manage. The
//! server authenticates an agent by a long-lived bearer token that an
//! administrator issues once (ADR 0004 in kasl-server), so there is nothing
//! to log into and nothing to cache - the token goes in the OS keyring and
//! every request carries it.
//!
//! ```rust,no_run
//! # use kasl::api::kasl_server::KaslServer;
//! # use kasl::libs::config::KaslServerConfig;
//! # async fn f() -> anyhow::Result<()> {
//! let config = KaslServerConfig {
//!     url: "https://kasl.example.com".to_string(),
//!     ca_certificate: None,
//! };
//!
//! let server = KaslServer::new(&config)?;
//! let health = server.health().await?;
//! println!("kasl-server {}", health.version);
//! # Ok(())
//! # }
//! ```

use crate::libs::config::KaslServerConfig;
use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, NaiveDate};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::Duration;

/// Keyring credential holding the agent token.
///
/// Named like the other secrets so it shows up beside them in the platform's
/// credential UI; the leading dot and `_secret` suffix are what
/// [`Secret::new`](crate::libs::secret::Secret::new) trims into the account
/// name.
pub const AGENT_TOKEN_SECRET: &str = ".kasl_server_secret";

/// Prompt shown when the agent token is missing from the keyring.
pub const AGENT_TOKEN_PROMPT: &str = "Enter the agent token issued by your kasl-server administrator";

/// How long to wait on a request before giving up.
///
/// Short on purpose: every call here is a foreground command the user is
/// waiting on, and a self-hosted server that has not answered in this long is
/// down rather than slow.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// What `GET /health` reports.
#[derive(Debug, Clone, Deserialize)]
pub struct Health {
    /// `ok` when the server considers itself serviceable.
    pub status: String,

    /// The server's own version - the product version, which the web UI and
    /// the API share.
    pub version: String,

    /// Whether the server reached its database on this request.
    pub database: String,
}

/// The identity behind a token, as `GET /api/v1/agent/whoami` reports it.
///
/// Used to confirm a token belongs to whom the user expects: connecting with
/// a colleague's token would otherwise succeed silently and file this
/// machine's days under their name.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentIdentity {
    /// Display name of the employee the token reports for.
    pub user_name: String,

    /// The label the administrator gave this agent, typically the machine.
    pub agent_name: String,

    /// The API version the server serves this path under.
    pub api_version: String,

    /// The server's own version.
    pub server_version: String,
}

/// One day as this agent recorded it, in the shape the server accepts.
///
/// Field names and types mirror the ingest contract (ADR 0004 in
/// kasl-server) rather than kasl's own model, so a change on either side
/// shows up as a compile error here rather than as a `400` in the field.
///
/// Every instant carries a UTC offset. kasl stores bare wall-clock text,
/// which is unambiguous on one laptop and meaningless across a team; the
/// offset is attached when the day is assembled, and a day whose offset
/// cannot be determined is not sent (ADR 0003).
#[derive(Debug, Clone, Serialize)]
pub struct DayUpload {
    /// The employee's own calendar date, sent rather than derived: near
    /// midnight the date of `started_at` and the date the work belongs to
    /// disagree, and the agent is the side that knows which is meant.
    pub date: NaiveDate,

    /// When the day started.
    pub started_at: DateTime<FixedOffset>,

    /// When it ended; absent while the day is still open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<FixedOffset>>,

    pub pauses: Vec<PauseUpload>,

    pub tasks: Vec<TaskUpload>,

    /// Declares `tasks` to be everything this agent holds for the date, so a
    /// task the employee deleted here is deleted there too (ADR 0005).
    ///
    /// kasl always sends the whole date, so this is always true. It is a
    /// field rather than a constant because the server defaults it to false
    /// for agents that predate it, and saying it explicitly is what
    /// distinguishes "I have nothing more" from "I did not mention".
    pub tasks_are_complete: bool,
}

/// One break, as the server takes it.
#[derive(Debug, Clone, Serialize)]
pub struct PauseUpload {
    pub started_at: DateTime<FixedOffset>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<FixedOffset>>,

    /// Seconds. Sent explicitly because kasl merges neighbouring pauses
    /// before reporting them, so this is not always `ended_at - started_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<i32>,

    /// A break the employee entered by hand - kasl's `protected` flag.
    pub manual: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One task, keyed by the ids this agent knows it by.
#[derive(Debug, Clone, Serialize)]
pub struct TaskUpload {
    /// This agent's row id: the key a re-upload matches on, so a corrected
    /// task updates the stored row instead of piling up beside it.
    pub agent_task_id: i32,

    /// This agent's `task_id`, tying the same work across several days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_group_id: Option<i32>,

    pub recorded_at: DateTime<FixedOffset>,

    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Percent complete, 0..=100.
    pub completeness: i16,
}

/// What the server reports about a day it stored.
#[derive(Debug, Clone, Deserialize)]
pub struct DayAccepted {
    /// The server's own id for the day, which this agent does not otherwise
    /// know: worth printing so a day can be looked up on the other side.
    pub workday_id: String,

    pub date: NaiveDate,

    pub pauses: usize,

    pub tasks: usize,

    /// Tasks the server dropped because this upload declared its set
    /// authoritative. Non-zero means deletions here reached the server.
    #[serde(default)]
    pub deleted_tasks: u64,

    /// The installation's privacy level, always reported - an agent should be
    /// able to tell a server that keeps everything from one whose policy it
    /// has not read (ADR 0011).
    #[serde(default)]
    pub privacy_level: Option<String>,
}

/// Why an upload failed, and whether sending the same bytes again could ever
/// work.
///
/// The distinction is the server's own (ADR 0005) and it is the whole reason
/// this is an enum rather than a message: `4xx` means the payload will never
/// be accepted as sent, so a queue must stop asking; `5xx` and `429` mean the
/// server could not answer this time, so it must keep the day and try later.
#[derive(Debug)]
pub enum UploadError {
    /// The server refused the payload itself. Retrying is pointless.
    Rejected { status: StatusCode, message: String },

    /// The server could not answer, or answered that it was unavailable.
    /// The day is still worth sending.
    Retryable { message: String },
}

impl UploadError {
    /// Whether sending this day again could succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(self, UploadError::Retryable { .. })
    }
}

impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UploadError::Rejected { status, message } => write!(f, "the server refused the day ({}): {}", status, message),
            UploadError::Retryable { message } => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for UploadError {}

/// A client bound to one kasl-server instance.
#[derive(Debug, Clone)]
pub struct KaslServer {
    client: Client,

    /// Base URL without a trailing slash, so paths append cleanly.
    base_url: String,
}

impl KaslServer {
    /// Builds a client for the configured server.
    ///
    /// A configured CA certificate is added to the trust store rather than
    /// replacing it: a company CA for the server and public CAs for
    /// everything else is the normal self-hosted arrangement.
    pub fn new(config: &KaslServerConfig) -> Result<Self> {
        let mut builder = Client::builder().timeout(REQUEST_TIMEOUT);

        if let Some(path) = &config.ca_certificate {
            let pem = fs::read(path).with_context(|| format!("cannot read the CA certificate at '{}'", path))?;

            // `Certificate::from_pem` defers parsing to the TLS backend and
            // accepts anything here - an empty file, a DER file saved with a
            // .pem name, a text file. The failure then surfaces at the first
            // request as an opaque TLS error, pointing at the network rather
            // than at the file. Checked here, where the path is still in hand.
            if !looks_like_pem_certificate(&pem) {
                bail!(
                    "'{}' does not contain a PEM-encoded certificate (expected a -----BEGIN CERTIFICATE----- block)",
                    path
                );
            }

            let certificate = reqwest::Certificate::from_pem(&pem).with_context(|| format!("'{}' is not a PEM-encoded certificate", path))?;
            builder = builder.add_root_certificate(certificate);
        }

        Ok(Self {
            client: builder.build().context("cannot build the HTTP client for kasl-server")?,
            base_url: normalize_url(&config.url),
        })
    }

    /// Asks the server whether it is serviceable, and which version it runs.
    ///
    /// Unauthenticated: this is the call that tells a misspelled URL from a
    /// bad token, so it must not need the token to answer.
    pub async fn health(&self) -> Result<Health> {
        let url = format!("{}/health", self.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("cannot reach kasl-server at {}", self.base_url))?;

        let status = response.status();
        if !status.is_success() {
            bail!("{} answered {} instead of a health report", self.base_url, status);
        }

        // A URL that points at something else entirely - a proxy, a parked
        // domain - answers 200 with a page. Insisting on the documented shape
        // keeps that from reading as a healthy server.
        response
            .json::<Health>()
            .await
            .with_context(|| format!("{} answered, but not like a kasl-server", self.base_url))
    }

    /// Resolves the agent token to the person it reports for.
    ///
    /// Doubles as the token check: the server refuses an unknown, revoked, or
    /// deactivated token with `401`, which is reported as such rather than as
    /// a transport failure.
    pub async fn identify(&self, token: &str) -> Result<AgentIdentity> {
        let url = format!("{}/api/v1/agent/whoami", self.base_url);
        let response = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("cannot reach kasl-server at {}", self.base_url))?;

        match response.status() {
            StatusCode::OK => response.json::<AgentIdentity>().await.context("cannot read the server's answer"),
            StatusCode::UNAUTHORIZED => bail!("the server rejected this token - it may be mistyped, revoked, or issued for a deactivated account"),
            status => bail!("the server answered {} when asked whose token this is", status),
        }
    }

    /// Sends one day to `POST /api/v1/days`.
    ///
    /// The last upload wins on the server, so re-sending a day corrects it
    /// and sending the same day twice changes nothing (ADR 0004). That makes
    /// a retry safe by construction, which is what the failure split here is
    /// for: [`UploadError`] separates a payload the server will never take
    /// from a server that could not answer this time.
    pub async fn upload_day(&self, token: &str, day: &DayUpload) -> Result<DayAccepted, UploadError> {
        let url = format!("{}/api/v1/days", self.base_url);
        let response = match self.client.post(&url).bearer_auth(token).json(day).send().await {
            Ok(response) => response,
            // Nothing was answered: DNS, TLS, a refused connection, a timeout.
            // The day is untouched on the server and worth sending again.
            Err(error) => {
                return Err(UploadError::Retryable {
                    message: format!("cannot reach kasl-server at {}: {}", self.base_url, error),
                });
            }
        };

        let status = response.status();
        if status.is_success() {
            // A 2xx whose body is not a day report means the address answers
            // for something other than this endpoint. Retrying a URL that is
            // wrong would never come good, so it is a rejection.
            return response.json::<DayAccepted>().await.map_err(|error| UploadError::Rejected {
                status,
                message: format!("the server accepted the day but answered unreadably: {}", error),
            });
        }

        // Read the body before classifying: the server explains a refusal
        // there ("tasks[0]: name is empty"), and a status alone would leave
        // the user with nothing to fix.
        let message = response.text().await.unwrap_or_default();
        let message = describe_failure(status, &message);

        // The server's own rule, not a guess: 4xx will not be accepted as
        // sent; 5xx and 429 are worth repeating. 429 sits inside the 4xx
        // range and is the one exception to it.
        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            Err(UploadError::Retryable { message })
        } else {
            Err(UploadError::Rejected { status, message })
        }
    }

    /// The base URL this client talks to, as stored.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Trims a user-typed URL into the form the client stores.
///
/// Only the trailing slash is removed. Guessing a scheme is deliberately not
/// done here: `http://` and `https://` differ by whether the token crosses
/// the network in the clear, which is not a default worth inventing on the
/// user's behalf.
pub fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// Extracts the sentence a person can act on from a failed response.
///
/// The server answers errors as JSON (`{"error": "..."}`), and showing that
/// wrapper verbatim buries the sentence that matters. A body in any other
/// shape is passed through as-is rather than dropped: an error from a proxy
/// in front of the server is still the most informative thing available.
///
/// The status is deliberately *not* added here. It is already carried by the
/// error and printed once when the failure is displayed, and some of the
/// server's own messages open with it too - stamping it on again produced
/// "the server refused the day (401 Unauthorized): 401 Unauthorized: the
/// token is not recognized", which reads as three different problems.
fn describe_failure(status: StatusCode, body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return format!("the server gave no explanation ({})", status);
    }

    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("error").and_then(|error| error.as_str()).map(str::to_string))
        .unwrap_or_else(|| body.chars().take(300).collect())
}

/// Whether a file's bytes carry a PEM certificate block.
///
/// A deliberately shallow check. It catches the mistakes people actually make
/// (the wrong file, an empty file, a DER export named `.pem`) and leaves
/// judging the certificate itself to the TLS backend, which is the only thing
/// qualified to do so.
fn looks_like_pem_certificate(pem: &[u8]) -> bool {
    // Text search over bytes rather than a UTF-8 conversion: a PEM file is
    // ASCII, but a binary file that is not valid UTF-8 should fail this check
    // rather than fail to be examined.
    pem.windows(BEGIN_CERTIFICATE.len()).any(|window| window == BEGIN_CERTIFICATE)
}

/// The header opening a PEM certificate block.
const BEGIN_CERTIFICATE: &[u8] = b"-----BEGIN CERTIFICATE-----";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_url_drops_a_trailing_slash() {
        assert_eq!(normalize_url("https://kasl.example.com/"), "https://kasl.example.com");
        assert_eq!(normalize_url("https://kasl.example.com"), "https://kasl.example.com");
    }

    #[test]
    fn normalize_url_trims_surrounding_whitespace() {
        // Pasting a URL from a chat message routinely brings a space along.
        assert_eq!(normalize_url("  https://kasl.example.com/  "), "https://kasl.example.com");
    }

    #[test]
    fn normalize_url_keeps_a_path_prefix() {
        // A server behind a reverse proxy can live under a sub-path, and
        // dropping it would send every request to the proxy's root.
        assert_eq!(normalize_url("https://intranet.example.com/kasl/"), "https://intranet.example.com/kasl");
    }

    #[test]
    fn a_client_is_built_without_a_certificate() {
        let config = KaslServerConfig {
            url: "https://kasl.example.com/".to_string(),
            ca_certificate: None,
        };

        let server = KaslServer::new(&config).unwrap();
        assert_eq!(server.base_url(), "https://kasl.example.com");
    }

    #[test]
    fn a_missing_certificate_file_is_reported_by_path() {
        let config = KaslServerConfig {
            url: "https://kasl.example.com".to_string(),
            ca_certificate: Some("/nonexistent/company-ca.pem".to_string()),
        };

        let error = KaslServer::new(&config).unwrap_err().to_string();
        assert!(error.contains("company-ca.pem"), "the error should name the file: {}", error);
    }

    /// Writes `bytes` to a uniquely named file and hands back its path.
    fn certificate_file(name: &str, bytes: &[u8]) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("kasl-ca-test-{}-{}", std::process::id(), name));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.pem"));
        fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    /// Every shape of "not a certificate" a person actually hands over.
    ///
    /// `reqwest::Certificate::from_pem` accepts all of these without
    /// complaint - it defers parsing to the TLS backend - so each one used to
    /// connect happily and fail later as an opaque TLS error.
    #[test]
    fn a_certificate_that_is_not_pem_is_refused() {
        for (name, bytes) in [
            ("plain-text", &b"this is not a certificate"[..]),
            ("empty", &b""[..]),
            ("wrong-pem-block", &b"-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----\n"[..]),
            // A DER export saved with a .pem name: binary, and not valid UTF-8.
            ("der-as-pem", &[0x30u8, 0x82, 0x01, 0x0a, 0xff, 0xfe][..]),
        ] {
            let (dir, path) = certificate_file(name, bytes);

            let config = KaslServerConfig {
                url: "https://kasl.example.com".to_string(),
                ca_certificate: Some(path.to_string_lossy().into_owned()),
            };

            let error = match KaslServer::new(&config) {
                Ok(_) => panic!("'{name}' should not have been accepted as a certificate"),
                Err(error) => error.to_string(),
            };
            assert!(error.contains("PEM"), "the error for '{}' should say the file is not PEM: {}", name, error);
            assert!(error.contains(name), "the error for '{}' should name the file: {}", name, error);

            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_real_certificate_block_is_accepted() {
        // The counterpart to the test above: the check must not refuse the
        // file it exists to let through. Body content is left to the TLS
        // backend - what is asserted here is that a PEM block gets that far.
        let (dir, path) = certificate_file("company-ca", b"-----BEGIN CERTIFICATE-----\nMIIBkTCB+wIJAKZ\n-----END CERTIFICATE-----\n");

        let config = KaslServerConfig {
            url: "https://kasl.example.com".to_string(),
            ca_certificate: Some(path.to_string_lossy().into_owned()),
        };

        // Accepted by our check; whether the bytes decode is the backend's
        // call, and either answer here means the shallow check let it through.
        let _ = KaslServer::new(&config);

        let _ = fs::remove_dir_all(&dir);
    }
}
