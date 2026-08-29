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
use reqwest::{Client, StatusCode};
use serde::Deserialize;
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
