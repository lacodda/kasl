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

use crate::api::kasl_server::{AGENT_TOKEN_PROMPT, AGENT_TOKEN_SECRET, KaslServer, UploadError, normalize_url};
use crate::db::server_outbox::ServerOutbox;
use crate::db::workdays::Workdays;
use crate::libs::config::{Config, KaslServerConfig};
use crate::libs::day_delivery::{Delivered, deliver, record_single};
use crate::libs::day_upload::build_day_upload;
use crate::libs::messages::Message;
use crate::libs::secret::Secret;
use crate::{msg_error_anyhow, msg_info, msg_print, msg_success, msg_warning};
use anyhow::{Context, Result};
use chrono::{Duration, Local, NaiveDate};
use clap::{Args, Subcommand};
use dialoguer::{Input, Password, theme::ColorfulTheme};
use reqwest::StatusCode;

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

    /// Send a day to the server
    #[command(about = "Send a day's work to the connected kasl-server")]
    Push(PushArgs),

    /// Send everything that is still owed
    #[command(about = "Send every day still waiting to reach the server")]
    Flush,

    /// Show what is still waiting to be sent
    #[command(about = "Show the days still waiting to reach the server")]
    Queue,

    /// Queue a stretch of past days
    #[command(about = "Queue every recorded day in a date range and send them")]
    Backfill(BackfillArgs),

    /// Forget the connection and the stored token
    #[command(about = "Forget the connection and the stored agent token")]
    Disconnect,
}

/// Arguments accepted by `kasl server backfill`.
#[derive(Debug, Args)]
pub struct BackfillArgs {
    /// First date of the range, YYYY-MM-DD
    #[arg(long, value_name = "YYYY-MM-DD")]
    from: NaiveDate,

    /// Last date of the range, YYYY-MM-DD; defaults to today
    #[arg(long, value_name = "YYYY-MM-DD")]
    to: Option<NaiveDate>,
}

/// Arguments accepted by `kasl server push`.
#[derive(Debug, Args)]
pub struct PushArgs {
    /// Send yesterday instead of today
    #[arg(long, short, help = "Send the last day instead of today")]
    last: bool,

    /// Send a specific date, YYYY-MM-DD
    #[arg(long, value_name = "YYYY-MM-DD", conflicts_with = "last")]
    date: Option<NaiveDate>,
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
        ServerCommand::Push(args) => push(args).await,
        ServerCommand::Flush => flush().await,
        ServerCommand::Queue => queue(),
        ServerCommand::Backfill(args) => backfill(args).await,
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

/// Sends one day's work to the connected server.
///
/// The whole day goes every time - workday bounds, pauses, tasks - because
/// the server stores a day as a unit and the last upload wins (ADR 0004 in
/// kasl-server). Sending the same day twice therefore changes nothing, and a
/// day corrected here corrects itself there on the next push.
///
/// The day is assembled before the token is fetched: a day that cannot be
/// built - a timestamp with no valid offset, a task without an id - is a local
/// problem, and reporting it without first touching the keyring or the network
/// keeps the cause visible.
///
/// A day that cannot be delivered is queued rather than lost, and a day that
/// is delivered takes the rest of the backlog with it - a laptop coming back
/// from a week offline pays the whole debt on the first push, without the user
/// having to know a queue exists.
async fn push(args: PushArgs) -> Result<()> {
    let date = match args.date {
        Some(date) => date,
        None if args.last => (Local::now() - Duration::days(1)).date_naive(),
        None => Local::now().date_naive(),
    };

    let config = Config::read().unwrap_or_default();
    let Some(server_config) = config.kasl_server else {
        return Err(msg_error_anyhow!(Message::KaslServerNotConnected));
    };

    let Some(day) = build_day_upload(date)? else {
        msg_print!(Message::KaslServerNoDayToPush(date.to_string()));
        return Ok(());
    };

    let secret = Secret::new(AGENT_TOKEN_SECRET, AGENT_TOKEN_PROMPT);
    let Some(token) = secret.try_get_cached() else {
        return Err(msg_error_anyhow!(Message::KaslServerTokenMissing));
    };

    let client = KaslServer::new(&server_config)?;

    match client.upload_day(&token, &day).await {
        Ok(accepted) => {
            msg_success!(Message::KaslServerDayPushed {
                date: accepted.date.to_string(),
                pauses: accepted.pauses,
                tasks: accepted.tasks,
            });
            // Worth saying out loud rather than hiding in a debug log: this is
            // the visible consequence of declaring the task set authoritative,
            // and the only sign that a deletion here reached the server.
            if accepted.deleted_tasks > 0 {
                msg_info!(Message::KaslServerTasksDeleted(accepted.deleted_tasks));
            }

            // A day that arrives cancels its own debt. Without this a date
            // queued by an earlier failure would be sent again by the next
            // flush, forever.
            ServerOutbox::new()?.remove(date)?;

            // Today went, so the backlog is worth a try on the same
            // connection: a machine that comes back online typically owes
            // several days, and making the user run a second command to
            // discover that would be a queue that hides itself.
            flush_with(&client, &token).await?;
            Ok(())
        }
        // Three failures, three different things to do about them: a
        // credential to renew, a payload to fix, or a server to wait for.
        // Telling someone whose token was revoked to fix the day and push
        // again would send them looking at data that is not the problem.
        // All three are errors - the day did not arrive in any of them.
        Err(error) => {
            // Queued before it is reported, and only if a retry could ever
            // work: a day the server will never accept as sent would
            // otherwise sit in the queue retrying until someone noticed.
            let outcome = record_single(&mut ServerOutbox::new()?, date, &error)?;
            if matches!(outcome, Delivered::Deferred { .. }) {
                msg_info!(Message::KaslServerDayQueued(date.to_string()));
            }

            match error {
                error @ UploadError::Rejected {
                    status: StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN,
                    ..
                } => Err(msg_error_anyhow!(Message::KaslServerPushTokenRejected(error.to_string()))),
                error @ UploadError::Rejected { .. } => Err(msg_error_anyhow!(Message::KaslServerPushRejected(error.to_string()))),
                error => Err(msg_error_anyhow!(Message::KaslServerPushRetryable(error.to_string()))),
            }
        }
    }
}

/// Sends everything the outbox still owes.
async fn flush() -> Result<()> {
    let (client, token) = connected_client()?;

    if ServerOutbox::new()?.count()? == 0 {
        msg_print!(Message::KaslServerQueueEmpty);
        return Ok(());
    }

    flush_with(&client, &token).await
}

/// Drains the outbox against an already-built client.
///
/// Shared with `push` so a successful upload carries the backlog with it, on
/// the connection that was just proven to work.
async fn flush_with(client: &KaslServer, token: &str) -> Result<()> {
    let mut outbox = ServerOutbox::new()?;
    let dates: Vec<NaiveDate> = outbox.pending()?.into_iter().map(|owed| owed.date).collect();
    if dates.is_empty() {
        return Ok(());
    }

    msg_info!(Message::KaslServerQueueSending(dates.len()));

    let outcomes = deliver(client, token, &mut outbox, &dates).await?;

    let (mut accepted, mut refused, mut deferred) = (0, 0, 0);
    for outcome in &outcomes {
        match outcome {
            Delivered::Accepted {
                date,
                pauses,
                tasks,
                deleted_tasks,
            } => {
                accepted += 1;
                msg_success!(Message::KaslServerDayPushed {
                    date: date.to_string(),
                    pauses: *pauses,
                    tasks: *tasks,
                });
                if *deleted_tasks > 0 {
                    msg_info!(Message::KaslServerTasksDeleted(*deleted_tasks));
                }
            }
            // Named rather than counted: a day dropped because the server
            // will never take it is data that is not going to arrive, and
            // burying that in a total would be the queue losing a day
            // quietly.
            Delivered::Refused { date, reason } => {
                refused += 1;
                msg_warning!(Message::KaslServerDayRefused {
                    date: date.to_string(),
                    reason: reason.clone(),
                });
            }
            Delivered::Deferred { date, reason } => {
                deferred += 1;
                msg_warning!(Message::KaslServerDayDeferred {
                    date: date.to_string(),
                    reason: reason.clone(),
                });
            }
        }
    }

    msg_print!(Message::KaslServerFlushSummary { accepted, refused, deferred });
    Ok(())
}

/// Lists what is still owed, without touching the network.
///
/// Deliberately offline: this is the command someone runs to find out whether
/// their work is safe, and it has to answer on a train.
fn queue() -> Result<()> {
    let outbox = ServerOutbox::new()?;
    let owed = outbox.pending()?;

    if owed.is_empty() {
        msg_print!(Message::KaslServerQueueEmpty);
        return Ok(());
    }

    msg_info!(Message::KaslServerQueueOwed(owed.len() as i64));
    for day in &owed {
        msg_print!(Message::KaslServerQueueEntry {
            date: day.date.to_string(),
            attempts: day.attempts,
            last_error: day.last_error.clone(),
        });
    }

    Ok(())
}

/// Queues every recorded day in a range and sends them.
///
/// The range is walked against the database rather than the calendar: only
/// dates that actually have a workday are queued, so a month containing
/// weekends and leave does not fill the outbox with days that were never
/// worked and can never be sent.
async fn backfill(args: BackfillArgs) -> Result<()> {
    let to = args.to.unwrap_or_else(|| Local::now().date_naive());
    if args.from > to {
        return Err(msg_error_anyhow!(Message::KaslServerBackfillOrderReversed));
    }

    let (client, token) = connected_client()?;

    let mut workdays = Workdays::new()?;
    let mut dates = Vec::new();
    let mut date = args.from;
    while date <= to {
        if workdays.fetch(date)?.is_some() {
            dates.push(date);
        }
        date += Duration::days(1);
    }

    if dates.is_empty() {
        msg_print!(Message::KaslServerBackfillNoDays {
            from: args.from.to_string(),
            to: to.to_string(),
        });
        return Ok(());
    }

    msg_info!(Message::KaslServerBackfillRange {
        from: args.from.to_string(),
        to: to.to_string(),
        days: dates.len(),
    });

    // Queued before they are sent, so an interrupted backfill is not lost: a
    // run cut off halfway leaves the rest owed rather than forgotten.
    let mut outbox = ServerOutbox::new()?;
    for date in &dates {
        outbox.enqueue(*date, "queued by backfill")?;
    }

    flush_with(&client, &token).await
}

/// The client and token for the configured server, or a message saying why
/// there is none.
///
/// Both failures are the same shape - nothing can be sent - and both have a
/// single fix, `kasl server connect`.
fn connected_client() -> Result<(KaslServer, String)> {
    let config = Config::read().unwrap_or_default();
    let Some(server_config) = config.kasl_server else {
        return Err(msg_error_anyhow!(Message::KaslServerNotConnected));
    };

    let Some(token) = Secret::new(AGENT_TOKEN_SECRET, AGENT_TOKEN_PROMPT).try_get_cached() else {
        return Err(msg_error_anyhow!(Message::KaslServerTokenMissing));
    };

    Ok((KaslServer::new(&server_config)?, token))
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
