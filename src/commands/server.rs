//! `kasl server`: connect this machine to a kasl-server, and see where it
//! stands.
//!
//! The server is the team's, not this machine's: an administrator issues an
//! agent token and hands it over, and connecting is pasting it here (ADR 0004
//! in kasl-server - there is no open enrolment to abuse). What this command
//! owns is everything around that: checking the server is really a
//! kasl-server before storing anything, putting the token in the OS keyring
//! rather than in a readable config file, and saying whose token it is out
//! loud, while a person is watching.
//!
//! That last check is the one worth spelling out. A token is an opaque string;
//! one pasted from the wrong chat window works perfectly and files this
//! machine's days under a colleague's name. The server is asked who it thinks
//! is connecting, and the answer is printed.

use crate::api::kasl_server::{AGENT_TOKEN_PROMPT, AGENT_TOKEN_SECRET, KaslServer, normalize_url};
use crate::libs::config::{Config, KaslServerConfig};
use crate::libs::messages::Message;
use crate::libs::secret::Secret;
use crate::{msg_error_anyhow, msg_info, msg_print, msg_success, msg_warning};
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use dialoguer::{Input, Password, theme::ColorfulTheme};

/// Command-line arguments for the server command.
#[derive(Debug, Args)]
pub struct ServerArgs {
    #[command(subcommand)]
    command: ServerCommand,
}

/// Available server operations.
#[derive(Debug, Subcommand)]
enum ServerCommand {
    /// Connect this machine to a kasl-server
    #[command(about = "Connect this machine to a kasl-server")]
    Connect(ConnectArgs),

    /// Show the current connection
    #[command(about = "Show the current connection to a kasl-server")]
    Status,

    /// Forget the connection and the stored token
    #[command(about = "Forget the connection and the stored agent token")]
    Disconnect,
}

/// Arguments accepted by `kasl server connect`.
#[derive(Debug, Args)]
pub struct ConnectArgs {
    /// Server URL, e.g. https://kasl.example.com; prompted for when omitted
    #[arg(long, value_name = "URL")]
    url: Option<String>,

    /// PEM file with the CA that signed the server's certificate
    #[arg(long, value_name = "PATH")]
    ca_certificate: Option<String>,
}

/// Routes a server subcommand.
pub async fn cmd(args: ServerArgs) -> Result<()> {
    match args.command {
        ServerCommand::Connect(args) => connect(args).await,
        ServerCommand::Status => status().await,
        ServerCommand::Disconnect => disconnect(),
    }
}

/// Walks through connecting: URL, token, and two checks against the server.
///
/// Nothing is written until both checks pass. A half-written connection - a
/// URL saved with a token the server never accepted - would leave the agent
/// looking configured while every upload failed.
async fn connect(args: ConnectArgs) -> Result<()> {
    // The token is always typed at a prompt - never taken from an argument,
    // where it would land in shell history - so this command cannot finish
    // without a terminal whatever else it was given. Checked before anything
    // else so a run that cannot succeed fails immediately, rather than after
    // reaching the network and reporting a server it is about to walk away
    // from.
    crate::libs::prompt::ensure_interactive("`kasl server connect` needs a terminal to ask for the agent token")?;

    let mut config = Config::read().unwrap_or_default();

    let url = match args.url {
        Some(url) => normalize_url(&url),
        None => {
            let entered: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::PromptKaslServerUrl.to_string())
                .with_initial_text(config.kasl_server.as_ref().map(|s| s.url.clone()).unwrap_or_default())
                .interact_text()?;
            normalize_url(&entered)
        }
    };

    // A URL without a scheme reaches nothing and the failure reads like the
    // server is down, so it is caught here where the cause is still visible.
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(msg_error_anyhow!(Message::KaslServerUrlNeedsScheme(url)));
    }

    let candidate = KaslServerConfig {
        url: url.clone(),
        // A certificate named now wins; otherwise an existing one is kept, so
        // reconnecting to the same server does not silently drop it.
        ca_certificate: args
            .ca_certificate
            .or_else(|| config.kasl_server.as_ref().and_then(|s| s.ca_certificate.clone())),
    };

    let client = KaslServer::new(&candidate)?;

    // First check: is this a kasl-server at all? Asked before the token, so a
    // mistyped URL is not reported as a rejected token.
    let health = client.health().await?;
    msg_info!(Message::KaslServerReached {
        url: url.clone(),
        version: health.version.clone(),
    });
    if health.database != "ok" {
        // Serviceable enough to answer, not enough to accept a day. Worth
        // saying now rather than at the first upload.
        msg_warning!(Message::KaslServerDatabaseUnhealthy(health.database.clone()));
    }

    let secret = Secret::new(AGENT_TOKEN_SECRET, AGENT_TOKEN_PROMPT);
    let token: String = Password::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptKaslServerToken.to_string())
        .interact()?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(msg_error_anyhow!(Message::KaslServerTokenEmpty));
    }

    // Second check: the server accepts this token, and says whose it is.
    let identity = client.identify(&token).await?;

    // Both checks passed - only now is anything persisted.
    secret
        .store(&token)
        .context("the token was accepted but could not be stored in the OS keyring")?;
    config.kasl_server = Some(candidate);
    config.save()?;

    msg_success!(Message::KaslServerConnected {
        user_name: identity.user_name,
        agent_name: identity.agent_name,
    });
    Ok(())
}

/// Reports the stored connection, and whether it still works.
///
/// Reaches the server rather than reading the config back: a connection that
/// was valid when it was made and is not any more - a revoked token, a server
/// that moved - is exactly what someone runs this to find out.
async fn status() -> Result<()> {
    let config = Config::read().unwrap_or_default();
    let Some(server_config) = config.kasl_server else {
        msg_print!(Message::KaslServerNotConnected);
        return Ok(());
    };

    msg_info!(Message::KaslServerConfigured(server_config.url.clone()));

    let secret = Secret::new(AGENT_TOKEN_SECRET, AGENT_TOKEN_PROMPT);
    let Some(token) = secret.try_get_cached() else {
        // The config says connected and the keyring disagrees: reconnecting is
        // the fix, and saying so beats a 401 at the next upload.
        msg_warning!(Message::KaslServerTokenMissing);
        return Ok(());
    };

    let client = KaslServer::new(&server_config)?;

    match client.health().await {
        Ok(health) => msg_info!(Message::KaslServerReached {
            url: server_config.url.clone(),
            version: health.version,
        }),
        Err(error) => {
            msg_warning!(Message::KaslServerUnreachable(error.to_string()));
            return Ok(());
        }
    }

    match client.identify(&token).await {
        Ok(identity) => msg_success!(Message::KaslServerConnected {
            user_name: identity.user_name,
            agent_name: identity.agent_name,
        }),
        Err(error) => msg_warning!(Message::KaslServerTokenRejected(error.to_string())),
    }

    Ok(())
}

/// Forgets the connection: the token first, then the config.
///
/// In that order deliberately. If removing the config succeeded and the token
/// removal then failed, a working credential would be left behind with nothing
/// pointing at it - the one outcome this command exists to prevent.
fn disconnect() -> Result<()> {
    let mut config = Config::read().unwrap_or_default();

    // An unreachable keyring must not stop the address being forgotten. On a
    // headless machine - a container, a build agent, a server with no session
    // keyring - there is no store to hold a token and nothing to remove, and
    // refusing to disconnect there leaves the config pointing at a server for
    // good. The failure is still reported, because on a machine that does have
    // a keyring it means a credential survived.
    if let Err(error) = Secret::new(AGENT_TOKEN_SECRET, AGENT_TOKEN_PROMPT).delete() {
        msg_warning!(Message::KaslServerTokenNotRemoved(error.to_string()));
    }

    if config.kasl_server.take().is_some() {
        config.save()?;
        msg_success!(Message::KaslServerDisconnected);
    } else {
        // The token is gone either way, which is what was asked for.
        msg_print!(Message::KaslServerNotConnected);
    }

    Ok(())
}
