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
//! The pulse is the one part that is not about days: whether this person is
//! working right now. It is turned on separately (`kasl server pulse
//! enable`), because connecting agrees to send finished days and a live
//! signal is a different thing to agree to. The watcher sends it; this
//! command only switches it and reports on it.
//!
//! That last check is the one worth spelling out. A token is an opaque string;
//! one pasted from the wrong chat window works perfectly and files this
//! machine's days under a colleague's name. The server is asked who it thinks
//! is connecting, and the answer is printed.

use crate::api::kasl_server::{AGENT_TOKEN_PROMPT, AGENT_TOKEN_SECRET, KaslServer, UploadError, normalize_url};
use crate::db::server_outbox::ServerOutbox;
use crate::db::server_pulse::ServerPulse;
use crate::db::workdays::Workdays;
use crate::libs::config::{Config, KaslServerConfig};
use crate::libs::daemon;
use crate::libs::day_delivery::{Delivered, deliver, record_single};
use crate::libs::day_upload::build_day_upload;
use crate::libs::messages::Message;
use crate::libs::pulse::{self, format_ago};
use crate::libs::secret::Secret;
use crate::{msg_error_anyhow, msg_info, msg_print, msg_success, msg_warning};
use anyhow::{Context, Result};
use chrono::{Duration, Local, NaiveDate, Utc};
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

    /// Show what this server keeps about you
    #[command(about = "Show what the connected kasl-server stores about you")]
    Manifest,

    /// Queue a stretch of past days
    #[command(about = "Queue every recorded day in a date range and send them")]
    Backfill(BackfillArgs),

    /// Tell the server whether you are working right now
    #[command(about = "Turn the live pulse on or off, or show how it is going")]
    Pulse(PulseArgs),

    /// Forget the connection and the stored token
    #[command(about = "Forget the connection and the stored agent token")]
    Disconnect,
}

/// Arguments accepted by `kasl server backfill`.
#[derive(Debug, Args)]
pub struct BackfillArgs {
    /// First date of the range, YYYY-MM-DD; without it, the whole history
    #[arg(long, value_name = "YYYY-MM-DD")]
    from: Option<NaiveDate>,

    /// Last date of the range, YYYY-MM-DD; defaults to today
    #[arg(long, value_name = "YYYY-MM-DD")]
    to: Option<NaiveDate>,
}

/// Arguments accepted by `kasl server pulse`.
#[derive(Debug, Args)]
pub struct PulseArgs {
    #[command(subcommand)]
    command: Option<PulseCommand>,
}

/// What `kasl server pulse` can do; without one, it shows how the pulse is
/// going.
#[derive(Debug, Subcommand)]
enum PulseCommand {
    /// Start telling the server whether you are working right now
    #[command(about = "Start telling the server whether you are working, on a break, or not in a day")]
    Enable,

    /// Stop telling it
    #[command(about = "Stop telling the server whether you are working right now")]
    Disable,
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
        ServerCommand::Manifest => manifest().await,
        ServerCommand::Backfill(args) => backfill(args).await,
        ServerCommand::Pulse(args) => match args.command {
            Some(PulseCommand::Enable) => pulse_enable().await,
            Some(PulseCommand::Disable) => pulse_disable(),
            None => pulse_show(),
        },
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
        pulse: keeps_pulse(config.kasl_server.as_ref(), &url),
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
    report_connection(&server_config).await?;

    // Last, and whatever the connection checks found: the pulse's record is
    // local, and a server that cannot be reached is exactly when "the last
    // one went 20 min ago" is worth reading.
    report_pulse(server_config.pulse)
}

/// The connection checks of `status`: the token, the server, whose token it is.
async fn report_connection(server_config: &KaslServerConfig) -> Result<()> {
    let secret = Secret::new(AGENT_TOKEN_SECRET, AGENT_TOKEN_PROMPT);
    let Some(token) = secret.try_get_cached() else {
        // The config says connected and the keyring disagrees: reconnecting is
        // the fix, and saying so beats a 401 at the next upload.
        msg_warning!(Message::KaslServerTokenMissing);
        return Ok(());
    };

    let client = KaslServer::new(server_config)?;

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
        Ok(identity) => {
            msg_success!(Message::KaslServerConnected {
                user_name: identity.user_name,
                agent_name: identity.agent_name,
            });
            // The two versions that decide whether this agent and that server
            // understand each other, printed where someone diagnosing a
            // refused upload is already looking. `whoami` has reported both
            // since kasl-server 0.14.1, which is also the floor `connect`
            // enforces, so an answer here means the pair is compatible - the
            // only way to reach this line at all is to have connected.
            msg_info!(Message::KaslServerCompatibility {
                server_version: identity.server_version,
                api_version: identity.api_version,
            });
        }
        Err(error) => msg_warning!(Message::KaslServerTokenRejected(error.to_string())),
    }

    Ok(())
}

/// Prints how the pulse is going, from what the watcher last recorded.
fn report_pulse(enabled: bool) -> Result<()> {
    let record = if enabled { ServerPulse::new()?.last()? } else { None };
    for line in pulse::describe(enabled, record.as_ref(), Utc::now()) {
        if line.warning {
            msg_warning!(line.message);
        } else {
            msg_info!(line.message);
        }
    }
    Ok(())
}

/// Whether a connection being made keeps the pulse the previous one had.
///
/// Only for the same server. The consent was given to one installation, and
/// reconnecting to another - a new employer, a test instance - must not start
/// reporting to it what the employee agreed to report somewhere else.
fn keeps_pulse(previous: Option<&KaslServerConfig>, url: &str) -> bool {
    previous.is_some_and(|previous| previous.pulse && previous.url == url)
}

/// How long to wait for the watcher's first pulse after turning it on.
///
/// A tick, a request and a margin. Long enough that a watcher that is running
/// answers inside it; short enough that one that is not costs little.
const FIRST_PULSE_WAIT: std::time::Duration = std::time::Duration::from_secs(12);

/// Turns the pulse on, then waits to see the first one go.
///
/// The watcher is the only sender, so this does not send a pulse of its own:
/// it clears the record, which is the watcher's cue to send at once, and reads
/// back what the watcher wrote. A second sender here would put two claims about
/// the present on the server, one of them made by a process that cannot see
/// whether this is a pause.
async fn pulse_enable() -> Result<()> {
    let mut config = Config::read().unwrap_or_default();
    let Some(server) = config.kasl_server.as_mut() else {
        return Err(msg_error_anyhow!(Message::KaslServerNotConnected));
    };
    // Checked now rather than discovered by the watcher: a pulse turned on
    // without a token would fail every five minutes where nobody is looking.
    if Secret::new(AGENT_TOKEN_SECRET, AGENT_TOKEN_PROMPT).try_get_cached().is_none() {
        return Err(msg_error_anyhow!(Message::KaslServerTokenMissing));
    }

    ServerPulse::new()?.clear()?;
    server.pulse = true;
    let url = server.url.clone();
    config.save()?;
    msg_success!(Message::PulseEnabled(url));

    if !daemon::is_running() {
        msg_warning!(Message::PulseNoWatcher);
        return Ok(());
    }

    let deadline = std::time::Instant::now() + FIRST_PULSE_WAIT;
    loop {
        if let Some(record) = ServerPulse::new()?.last()? {
            match (record.error, record.state) {
                (None, Some(state)) => msg_success!(Message::PulseFirstArrived(state.to_string())),
                (Some(error), _) => msg_warning!(Message::PulseFirstFailed(error)),
                (None, None) => msg_info!(Message::PulseFirstPending),
            }
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            msg_info!(Message::PulseFirstPending);
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

/// Turns the pulse off.
///
/// Nothing is sent to say so: the server has no way to be told "stop", only
/// silence, which it shows as offline once the last pulse is too old to
/// believe. Saying that here is the honest part - the employee should not
/// expect the dashboard to go blank.
fn pulse_disable() -> Result<()> {
    let mut config = Config::read().unwrap_or_default();
    let Some(server) = config.kasl_server.as_mut() else {
        msg_print!(Message::KaslServerNotConnected);
        return Ok(());
    };
    if !server.pulse {
        msg_print!(Message::PulseAlreadyOff);
        return Ok(());
    }

    server.pulse = false;
    config.save()?;

    let mut store = ServerPulse::new()?;
    let stale_after = store.last()?.and_then(|record| record.stale_after_seconds).map(format_ago);
    store.clear()?;

    msg_success!(Message::PulseDisabled(stale_after));
    Ok(())
}

/// Shows how the pulse is going, without touching the network.
fn pulse_show() -> Result<()> {
    let config = Config::read().unwrap_or_default();
    let Some(server) = config.kasl_server else {
        msg_print!(Message::KaslServerNotConnected);
        return Ok(());
    };
    report_pulse(server.pulse)
}

/// Prints what the connected server stores about this person.
///
/// Read from the server every time rather than described from here. The
/// manifest is generated on the server out of the level it enforces at ingest
/// (ADR 0011 in kasl-server), so it describes the installation this machine
/// actually reports to; a copy kept in kasl would describe the server kasl
/// was built against, and would be most wrong exactly when it mattered - on
/// an installation that had narrowed what it keeps.
///
/// Showing is all this command does. The level belongs to the installation
/// and an administrator sets it; there is no personal opt-out (ADR 0011), and
/// a flag here that appeared to narrow it would be a promise kasl cannot
/// keep.
async fn manifest() -> Result<()> {
    let (client, token) = connected_client()?;

    let manifest = client.privacy(&token).await?;

    msg_info!(Message::KaslServerPrivacyHeading(manifest.level));
    msg_print!(Message::KaslServerPrivacySummary(manifest.summary));

    msg_print!(Message::KaslServerPrivacyStoredHeading);
    for stored in manifest.stored {
        msg_print!(Message::KaslServerPrivacyStored {
            what: stored.what,
            detail: stored.detail,
        });
    }

    msg_print!(Message::KaslServerPrivacyNeverHeading);
    for line in manifest.never_collected {
        msg_print!(Message::KaslServerPrivacyBullet(line));
    }

    msg_print!(Message::KaslServerPrivacyVisibleHeading);
    for line in manifest.visible_to {
        msg_print!(Message::KaslServerPrivacyBullet(line));
    }

    msg_print!(Message::KaslServerPrivacyRetention(manifest.retention));
    msg_print!(Message::KaslServerPrivacyOnChange(manifest.on_change));

    // Only when the server says. An absent timestamp means this server does
    // not record when the level was set, which is not the same as a level
    // that was never changed - printing "never" for it would invent a fact.
    if let Some(updated_at) = manifest.updated_at {
        msg_print!(Message::KaslServerPrivacyUpdatedAt(
            updated_at.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()
        ));
    }

    msg_print!(Message::KaslServerPrivacySetByAdmin);

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
    // What is owed is checked before the connection is, because an empty queue
    // is nothing to do whatever the connection looks like. The other order
    // makes an hourly `kasl server flush` on an unconnected machine fail every
    // hour over work that does not exist - and a cron job that cries wolf is
    // one nobody reads by the time it matters.
    if ServerOutbox::new()?.count()? == 0 {
        msg_print!(Message::KaslServerQueueEmpty);
        return Ok(());
    }

    let (client, token) = connected_client()?;
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
/// The range is read out of the database rather than walked across the
/// calendar: only dates that actually have a workday are queued, so a month
/// containing weekends and leave does not fill the outbox with days that were
/// never worked and can never be sent.
///
/// Without `--from` the range opens at the first day this machine ever
/// recorded. That is the honest default for what the command is for - a
/// machine that tracked locally before the team had a server owes everything,
/// and asking the employee to first find out when they started using kasl in
/// order to say so is asking the database's own question back at them.
async fn backfill(args: BackfillArgs) -> Result<()> {
    let to = args.to.unwrap_or_else(|| Local::now().date_naive());
    if let Some(from) = args.from
        && from > to
    {
        return Err(msg_error_anyhow!(Message::KaslServerBackfillOrderReversed));
    }

    let (client, token) = connected_client()?;

    let dates = Workdays::new()?.recorded_dates(args.from, Some(to))?;

    if dates.is_empty() {
        match args.from {
            Some(from) => msg_print!(Message::KaslServerBackfillNoDays {
                from: from.to_string(),
                to: to.to_string(),
            }),
            // A different sentence on purpose. "No workdays between the
            // beginning and today" would read as a range that happened to be
            // empty; what it actually means is that this machine has never
            // recorded a day, and the fix is to start one rather than to pick
            // other dates.
            None => msg_print!(Message::KaslServerBackfillNothingRecorded),
        }
        return Ok(());
    }

    match args.from {
        Some(from) => msg_info!(Message::KaslServerBackfillRange {
            from: from.to_string(),
            to: to.to_string(),
            days: dates.len(),
        }),
        // The first recorded date is named rather than left as "the
        // beginning": it is the one fact that tells the employee how much of
        // their history is about to reach the server, which is the thing
        // worth knowing before it does.
        None => msg_info!(Message::KaslServerBackfillWholeHistory {
            from: dates[0].to_string(),
            to: to.to_string(),
            days: dates.len(),
        }),
    }

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
        // The pulse's consent went with the connection; its record goes too,
        // so a later connection does not start out reporting an old one.
        ServerPulse::new()?.clear()?;
        msg_success!(Message::KaslServerDisconnected);
    } else {
        // The token is gone either way, which is what was asked for.
        msg_print!(Message::KaslServerNotConnected);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(url: &str, pulse: bool) -> KaslServerConfig {
        KaslServerConfig {
            url: url.to_string(),
            ca_certificate: None,
            pulse,
        }
    }

    #[test]
    fn reconnecting_to_the_same_server_keeps_the_pulse() {
        assert!(keeps_pulse(Some(&connection("https://kasl.example.com", true)), "https://kasl.example.com"));
    }

    #[test]
    fn another_server_does_not_inherit_the_consent() {
        // Agreeing to tell one installation whether you are at your desk is
        // not agreeing to tell the next one you connect to.
        assert!(!keeps_pulse(Some(&connection("https://kasl.example.com", true)), "https://kasl.other.example"));
        assert!(!keeps_pulse(Some(&connection("https://kasl.example.com", false)), "https://kasl.example.com"));
        assert!(!keeps_pulse(None, "https://kasl.example.com"));
    }
}
