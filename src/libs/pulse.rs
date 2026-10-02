//! The pulse: the watcher telling kasl-server whether you are working, on a
//! break, or not in a day - right now, once a minute.
//!
//! Everything else kasl sends is about the past: a day goes up when it is
//! pushed. The pulse is the one thing about the present, and the only side
//! that knows it is this one - the watcher knows whether it is inside a pause
//! and whether today's workday is open (ADR 0014 in kasl-server).
//!
//! Three rules shape it:
//!
//! * **Opt-in, per connection.** Connecting agrees to send finished days. A
//!   live signal of whether someone is at their keyboard is a different thing
//!   to agree to, so it waits for `kasl server pulse enable`.
//! * **The state and nothing else.** Not the task, not the reason for a
//!   break: those belong to the day, under the server's privacy level.
//! * **The server owns the cadence.** It answers every pulse with the interval
//!   it wants and the point at which it stops believing one; the two have to
//!   agree, so kasl takes both rather than configuring its own.
//!
//! The watcher is the only sender. `kasl server pulse enable` and `kasl server
//! status` read what it recorded in [`ServerPulse`] rather than sending a
//! pulse of their own, so the server never hears two versions of the present.
//!
//! ```rust,no_run
//! # async fn f(monitor: kasl::libs::monitor::Monitor) {
//! // Beside the monitor, for as long as the watcher runs.
//! tokio::spawn(kasl::libs::pulse::run(monitor.pause_flag()));
//! # }
//! ```

use crate::api::kasl_server::{AGENT_TOKEN_PROMPT, AGENT_TOKEN_SECRET, AgentState, KaslServer, Pulse, UploadError};
use crate::db::server_pulse::{PulseRecord, ServerPulse};
use crate::db::workdays::{Workday, Workdays};
use crate::libs::config::{Config, KaslServerConfig};
use crate::libs::messages::Message;
use crate::libs::secret::Secret;
use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

/// The interval used until the server has said what it wants.
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(60);

/// How long to wait after the server refused a pulse.
///
/// A refusal - a revoked token, a server older than the route, a clock too
/// far ahead - does not come good in a minute, and asking every minute would
/// only fill the server's log. Turning the pulse on again asks at once, so a
/// person who has just fixed the cause does not wait out the rest.
pub const REFUSED_RETRY: Duration = Duration::from_secs(5 * 60);

/// The narrowest and widest interval taken from a server.
///
/// A server is trusted with the cadence, not with a busy loop: zero, or an
/// hour, is a server answering wrongly rather than one with an opinion.
const INTERVAL_BOUNDS: (u64, u64) = (10, 15 * 60);

/// How often the watcher looks at what it would claim.
///
/// The pulse goes once an interval, but a change - into a pause, out of one -
/// goes as soon as it is seen. Looking costs a config read and one row from
/// the local database.
const TICK: Duration = Duration::from_secs(5);

/// How soon a re-enabled pulse may follow the last attempt.
///
/// Turning the pulse on clears its record, and a cleared record is the
/// watcher's cue to send at once - but never faster than this, so a record
/// that will not stay written cannot become a request every tick.
const RECORD_RETRY_GAP: Duration = Duration::from_secs(10);

/// How far past its next attempt the watcher may be before `status` calls it
/// silent. A tick and a slow answer, with room to spare.
const SILENCE_GRACE_SECONDS: i64 = 30;

/// A clock this far from the server's is worth a sentence in `status`.
///
/// Below it is network latency and ordinary drift. The server refuses a
/// pulse a minute ahead; saying so at ten seconds gives a person the chance
/// to fix the clock before that happens.
const SKEW_WORTH_MENTIONING_SECONDS: i64 = 10;

/// What the watcher claims, from what it sees.
///
/// "In a day" means today's workday is open: a day closed with `kasl end` is
/// over even if the keyboard is still busy. Inside an open day, a pause is a
/// break; outside one there is nothing to be on a break from.
pub fn presence(in_pause: bool, today: Option<&Workday>) -> AgentState {
    match today {
        Some(day) if day.end.is_none() => {
            if in_pause {
                AgentState::Paused
            } else {
                AgentState::Working
            }
        }
        _ => AgentState::Idle,
    }
}

/// The interval a server asked for, held inside [`INTERVAL_BOUNDS`].
pub fn interval_from(seconds: i64) -> Duration {
    let (low, high) = INTERVAL_BOUNDS;
    Duration::from_secs((seconds.max(0) as u64).clamp(low, high))
}

/// When the next pulse is due.
///
/// Due once an interval, and early when the claim changes - a dashboard that
/// learned about a break a minute late would be a minute wrong at exactly the
/// moments it is looked at. A failed attempt holds that back: until the wait
/// is over a change waits too, or a laptop on a train would try again at
/// every pause.
#[derive(Debug, Default, Clone)]
pub struct Cadence {
    next_at: Option<Instant>,
    attempted_at: Option<Instant>,
    sent_state: Option<AgentState>,
    holding: bool,
}

impl Cadence {
    /// Whether to send now, claiming `state`.
    ///
    /// `record_missing` is the pulse's record found empty: nobody has reported
    /// since the pulse was turned on, and the person who turned it on is
    /// waiting to hear how it went.
    pub fn is_due(&self, now: Instant, state: AgentState, record_missing: bool) -> bool {
        let Some(next_at) = self.next_at else {
            return true;
        };
        if now >= next_at {
            return true;
        }
        if !self.holding && self.sent_state != Some(state) {
            return true;
        }
        record_missing && self.attempted_at.is_none_or(|at| now.duration_since(at) >= RECORD_RETRY_GAP)
    }

    /// Notes a pulse the server took.
    pub fn sent(&mut self, now: Instant, state: AgentState, wait: Duration) {
        self.attempted_at = Some(now);
        self.next_at = Some(now + wait);
        self.sent_state = Some(state);
        self.holding = false;
    }

    /// Notes an attempt that did not arrive; nothing goes before `wait` is up.
    pub fn failed(&mut self, now: Instant, wait: Duration) {
        self.attempted_at = Some(now);
        self.next_at = Some(now + wait);
        self.holding = true;
    }
}

/// What came of one pulse.
#[derive(Debug, Clone, PartialEq)]
pub enum Beat {
    /// The server took it, and recorded this state.
    Accepted(AgentState),

    /// The server could not answer; worth trying at the next interval.
    Deferred(String),

    /// The server will not take it as sent; worth trying much later.
    Refused(String),
}

/// Sends one pulse claiming `state`, records how it went, and says how long
/// to wait before the next.
///
/// `interval` is the cadence in force - the server's last answer - and is
/// what a pulse that could not be delivered waits.
pub async fn beat(client: &KaslServer, token: &str, state: AgentState, interval: Duration, store: &mut ServerPulse) -> Result<(Beat, Duration)> {
    let at = Local::now().fixed_offset();
    let now = at.with_timezone(&Utc);

    let (beat, wait) = match client.heartbeat(token, &Pulse { state, at }).await {
        Ok(accepted) => {
            let wait = interval_from(accepted.interval_seconds);
            store.record_accepted(now, now + wait, accepted.state, accepted.stale_after_seconds, accepted.clock_skew_seconds)?;
            (Beat::Accepted(accepted.state), wait)
        }
        Err(UploadError::Retryable { message }) => {
            store.record_failed(now, now + interval, &message)?;
            (Beat::Deferred(message), interval)
        }
        Err(UploadError::Rejected { status, message }) => {
            let message = format!("the server refused the pulse ({}): {}", status, message);
            store.record_failed(now, now + REFUSED_RETRY, &message)?;
            (Beat::Refused(message), REFUSED_RETRY)
        }
    };

    Ok((beat, wait))
}

/// Where the agent token comes from.
type TokenSource = Box<dyn Fn() -> Option<String> + Send + Sync>;

/// The watcher's side of the pulse, one tick at a time.
///
/// Everything is re-read on each tick - the config, today's workday - so
/// `kasl server pulse enable` and `disable` take effect within a tick, with no
/// restart and no second channel into the watcher.
pub struct Pulser {
    in_pause: Arc<AtomicBool>,
    token: TokenSource,
    cadence: Cadence,
    interval: Duration,
    client: Option<(KaslServerConfig, KaslServer)>,
    workdays: Option<Workdays>,
    store: Option<ServerPulse>,
    last_error: Option<String>,
}

impl Pulser {
    /// A pulser reading the pause from `in_pause` and the token from the OS
    /// keyring.
    pub fn new(in_pause: Arc<AtomicBool>) -> Self {
        Self::with_token(in_pause, Box::new(|| Secret::new(AGENT_TOKEN_SECRET, AGENT_TOKEN_PROMPT).try_get_cached()))
    }

    /// A pulser taking its token from `token` - the keyring is not reachable
    /// from every test runner.
    pub fn with_token(in_pause: Arc<AtomicBool>, token: TokenSource) -> Self {
        Self {
            in_pause,
            token,
            cadence: Cadence::default(),
            interval: DEFAULT_INTERVAL,
            client: None,
            workdays: None,
            store: None,
            last_error: None,
        }
    }

    /// Looks once, and sends a pulse if one is due. Returns what came of it,
    /// or `None` when nothing was sent.
    pub async fn tick(&mut self) -> Result<Option<Beat>> {
        let config = Config::read()?;
        let Some(server) = config.kasl_server.filter(|server| server.pulse) else {
            // Off, or not connected. The cadence goes with it, so turning the
            // pulse on sends at once instead of waiting out an old interval.
            self.cadence = Cadence::default();
            self.client = None;
            return Ok(None);
        };

        let state = self.claim()?;
        let record_missing = self.store()?.last()?.is_none();
        let now = Instant::now();
        if !self.cadence.is_due(now, state, record_missing) {
            return Ok(None);
        }

        let client = match self.client_for(&server) {
            Ok(client) => client,
            // A configured certificate that cannot be read: no request can be
            // built, so this is a refusal of ours rather than the server's.
            Err(error) => return self.give_up(now, error.to_string()).map(Some),
        };

        let Some(token) = (self.token)() else {
            return self
                .give_up(now, "no agent token is stored - run `kasl server connect` again".to_string())
                .map(Some);
        };

        let interval = self.interval;
        let outcome = beat(&client, &token, state, interval, self.store()?).await;
        let (beat, wait) = match outcome {
            Ok(result) => result,
            Err(error) => {
                // The record could not be written. Hold off all the same, so a
                // broken database does not turn into a request every tick.
                self.cadence.failed(now, interval);
                self.store = None;
                return Err(error);
            }
        };

        match &beat {
            Beat::Accepted(_) => {
                self.interval = wait;
                self.cadence.sent(now, state, wait);
                if self.last_error.take().is_some() {
                    debug!("Pulse: arriving again");
                }
            }
            Beat::Deferred(reason) | Beat::Refused(reason) => {
                self.cadence.failed(now, wait);
                self.note_failure(reason);
            }
        }

        Ok(Some(beat))
    }

    /// What to claim right now.
    fn claim(&mut self) -> Result<AgentState> {
        let today = Local::now().date_naive();
        let day = match self.workdays()?.fetch(today) {
            Ok(day) => day,
            Err(error) => {
                // A handle that failed once is not trusted again.
                self.workdays = None;
                return Err(error);
            }
        };
        Ok(presence(self.in_pause.load(Ordering::Relaxed), day.as_ref()))
    }

    /// Records a failure of this side's own, before any request.
    fn give_up(&mut self, now: Instant, reason: String) -> Result<Beat> {
        self.cadence.failed(now, REFUSED_RETRY);
        let at = Utc::now();
        self.store()?.record_failed(at, at + chrono::Duration::from_std(REFUSED_RETRY)?, &reason)?;
        self.note_failure(&reason);
        Ok(Beat::Refused(reason))
    }

    /// Logs a failure once, not once a minute for as long as it lasts.
    fn note_failure(&mut self, reason: &str) {
        if self.last_error.as_deref() != Some(reason) {
            warn!("Pulse did not arrive: {}", reason);
            self.last_error = Some(reason.to_string());
        }
    }

    fn client_for(&mut self, server: &KaslServerConfig) -> Result<KaslServer> {
        if let Some((built_for, client)) = &self.client
            && built_for == server
        {
            return Ok(client.clone());
        }
        let client = KaslServer::new(server)?;
        self.client = Some((server.clone(), client.clone()));
        Ok(client)
    }

    fn workdays(&mut self) -> Result<&mut Workdays> {
        if self.workdays.is_none() {
            self.workdays = Some(Workdays::new()?);
        }
        Ok(self.workdays.as_mut().expect("opened above"))
    }

    fn store(&mut self) -> Result<&mut ServerPulse> {
        if self.store.is_none() {
            self.store = Some(ServerPulse::new()?);
        }
        Ok(self.store.as_mut().expect("opened above"))
    }
}

/// Runs the pulse for as long as the watcher does.
///
/// Never returns and never fails: a pulse that cannot go is recorded where
/// `kasl server status` finds it, and the watcher's real work - recording the
/// day - must not stop because the team server is unreachable.
pub async fn run(in_pause: Arc<AtomicBool>) {
    let mut pulser = Pulser::new(in_pause);
    loop {
        match pulser.tick().await {
            Ok(Some(Beat::Accepted(state))) => debug!("Pulse sent: {}", state),
            Ok(_) => {}
            Err(error) => warn!("Pulse: {}", error),
        }
        tokio::time::sleep(TICK).await;
    }
}

/// One line of the pulse's report, and whether it is a warning.
#[derive(Debug)]
pub struct StatusLine {
    pub warning: bool,
    pub message: Message,
}

impl StatusLine {
    fn info(message: Message) -> Self {
        Self { warning: false, message }
    }

    fn warning(message: Message) -> Self {
        Self { warning: true, message }
    }
}

/// What `kasl server status` says about the pulse.
///
/// Built from the record alone, so it answers offline and needs no running
/// watcher to ask. Whether a watcher is sending at all is read from the record
/// too: one that is running moves `next_at` forward, one that is gone leaves
/// it behind - which holds for a background and a foreground watcher alike.
pub fn describe(enabled: bool, record: Option<&PulseRecord>, now: DateTime<Utc>) -> Vec<StatusLine> {
    if !enabled {
        return vec![StatusLine::info(Message::PulseOff)];
    }
    let Some(record) = record else {
        return vec![StatusLine::info(Message::PulseNothingYet)];
    };

    let ago = |at: DateTime<Utc>| format_ago((now - at).num_seconds());
    let mut lines = Vec::new();

    match &record.error {
        None => lines.push(StatusLine::info(Message::PulseLastSent {
            ago: ago(record.sent_at.unwrap_or(record.attempted_at)),
            state: record.state.map(|state| state.to_string()),
        })),
        Some(error) => {
            lines.push(StatusLine::warning(Message::PulseFailing {
                ago: ago(record.attempted_at),
                error: error.clone(),
            }));
            if let Some(sent_at) = record.sent_at {
                lines.push(StatusLine::info(Message::PulseLastArrived(ago(sent_at))));
            }
        }
    }

    if (now - record.next_at).num_seconds() > SILENCE_GRACE_SECONDS {
        lines.push(StatusLine::warning(Message::PulseSilent(ago(record.next_at))));
    }

    if let (Some(sent_at), Some(stale_after)) = (record.sent_at, record.stale_after_seconds)
        && (now - sent_at).num_seconds() > stale_after
    {
        lines.push(StatusLine::warning(Message::PulseShownOffline(format_ago(stale_after))));
    }

    if let Some(skew) = record.clock_skew_seconds
        && skew.abs() >= SKEW_WORTH_MENTIONING_SECONDS
    {
        lines.push(StatusLine::warning(Message::PulseClockSkew(skew)));
    }

    lines
}

/// A span of seconds the way a person reads it: `42 s`, `6 min`, `2 h 5 min`,
/// `3 d`.
///
/// Coarser as it grows, because nobody acts on the seconds of something three
/// hours old. A negative span - a clock set back since - reads as none.
pub fn format_ago(seconds: i64) -> String {
    let seconds = seconds.max(0);
    match seconds {
        0..60 => format!("{} s", seconds),
        60..3600 => format!("{} min", seconds / 60),
        3600..86400 => match (seconds % 3600) / 60 {
            0 => format!("{} h", seconds / 3600),
            minutes => format!("{} h {} min", seconds / 3600, minutes),
        },
        _ => format!("{} d", seconds / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime};

    fn day(end: Option<NaiveDateTime>) -> Workday {
        let date = NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
        Workday {
            id: 1,
            date,
            start: date.and_hms_opt(9, 0, 0).unwrap(),
            end,
        }
    }

    #[test]
    fn an_open_day_is_working_or_paused() {
        let open = day(None);
        assert_eq!(presence(false, Some(&open)), AgentState::Working);
        assert_eq!(presence(true, Some(&open)), AgentState::Paused);
    }

    #[test]
    fn no_day_or_a_closed_one_is_idle_whatever_the_monitor_says() {
        // A pause outside a day is not a break from anything, and a day closed
        // with `kasl end` is over even if the keyboard is still busy.
        let closed = day(Some(NaiveDate::from_ymd_opt(2026, 10, 2).unwrap().and_hms_opt(18, 0, 0).unwrap()));
        for in_pause in [false, true] {
            assert_eq!(presence(in_pause, None), AgentState::Idle);
            assert_eq!(presence(in_pause, Some(&closed)), AgentState::Idle);
        }
    }

    #[test]
    fn a_server_interval_is_taken_within_bounds() {
        assert_eq!(interval_from(60), Duration::from_secs(60));
        // Zero would be a busy loop and an hour a dashboard that is always
        // stale; both are a server answering wrongly.
        assert_eq!(interval_from(0), Duration::from_secs(INTERVAL_BOUNDS.0));
        assert_eq!(interval_from(-5), Duration::from_secs(INTERVAL_BOUNDS.0));
        assert_eq!(interval_from(3600), Duration::from_secs(INTERVAL_BOUNDS.1));
    }

    #[test]
    fn the_first_pulse_goes_at_once() {
        assert!(Cadence::default().is_due(Instant::now(), AgentState::Working, false));
    }

    #[test]
    fn an_unchanged_claim_waits_for_the_interval() {
        let start = Instant::now();
        let mut cadence = Cadence::default();
        cadence.sent(start, AgentState::Working, Duration::from_secs(60));

        assert!(!cadence.is_due(start + Duration::from_secs(59), AgentState::Working, false));
        assert!(cadence.is_due(start + Duration::from_secs(60), AgentState::Working, false));
    }

    #[test]
    fn a_changed_claim_goes_without_waiting() {
        // The dashboard is looked at when someone wonders where a colleague
        // went; a break reported a minute late is wrong at exactly that moment.
        let start = Instant::now();
        let mut cadence = Cadence::default();
        cadence.sent(start, AgentState::Working, Duration::from_secs(60));

        assert!(cadence.is_due(start + Duration::from_secs(5), AgentState::Paused, false));
    }

    #[test]
    fn after_a_failure_even_a_change_waits() {
        // Offline on a train, every pause would otherwise be another request
        // into a network that is not there.
        let start = Instant::now();
        let mut cadence = Cadence::default();
        cadence.sent(start, AgentState::Working, Duration::from_secs(60));
        cadence.failed(start + Duration::from_secs(60), Duration::from_secs(60));

        assert!(!cadence.is_due(start + Duration::from_secs(70), AgentState::Paused, false));
        assert!(cadence.is_due(start + Duration::from_secs(120), AgentState::Paused, false));
    }

    #[test]
    fn a_cleared_record_asks_again_but_not_every_tick() {
        // Turning the pulse on again is the person's "try now" - after a
        // refusal they have just fixed, waiting five minutes would read as
        // the fix not working.
        let start = Instant::now();
        let mut cadence = Cadence::default();
        cadence.failed(start, REFUSED_RETRY);

        assert!(!cadence.is_due(start + Duration::from_secs(5), AgentState::Working, true));
        assert!(cadence.is_due(start + RECORD_RETRY_GAP, AgentState::Working, true));
        assert!(!cadence.is_due(start + RECORD_RETRY_GAP, AgentState::Working, false));
    }

    #[test]
    fn spans_read_coarser_as_they_grow() {
        assert_eq!(format_ago(0), "0 s");
        assert_eq!(format_ago(42), "42 s");
        assert_eq!(format_ago(60), "1 min");
        assert_eq!(format_ago(359), "5 min");
        assert_eq!(format_ago(3600), "1 h");
        assert_eq!(format_ago(7500), "2 h 5 min");
        assert_eq!(format_ago(3 * 86400 + 5), "3 d");
        assert_eq!(format_ago(-30), "0 s");
    }

    fn at(seconds_before: i64, now: DateTime<Utc>) -> DateTime<Utc> {
        now - chrono::Duration::seconds(seconds_before)
    }

    fn accepted(now: DateTime<Utc>, sent_ago: i64) -> PulseRecord {
        PulseRecord {
            attempted_at: at(sent_ago, now),
            next_at: at(sent_ago - 60, now),
            sent_at: Some(at(sent_ago, now)),
            state: Some(AgentState::Working),
            stale_after_seconds: Some(180),
            clock_skew_seconds: Some(0),
            error: None,
        }
    }

    fn rendered(lines: &[StatusLine]) -> Vec<String> {
        lines.iter().map(|line| line.message.to_string()).collect()
    }

    #[test]
    fn off_is_one_line_and_names_the_way_on() {
        let lines = describe(false, None, Utc::now());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].message.to_string().contains("kasl server pulse enable"));
    }

    #[test]
    fn a_healthy_pulse_says_how_long_ago_and_what() {
        let now = Utc::now();
        let lines = describe(true, Some(&accepted(now, 42)), now);
        assert_eq!(rendered(&lines), vec!["Pulse: on - the last one went 42 s ago (working)."]);
        assert!(!lines[0].warning);
    }

    #[test]
    fn a_failure_is_a_warning_and_keeps_the_last_arrival() {
        let now = Utc::now();
        let mut record = accepted(now, 400);
        record.attempted_at = at(20, now);
        record.next_at = at(-40, now);
        record.error = Some("cannot reach kasl-server".to_string());

        let lines = describe(true, Some(&record), now);
        let text = rendered(&lines).join("\n");
        assert!(lines[0].warning);
        assert!(text.contains("20 s ago") && text.contains("cannot reach kasl-server"), "{text}");
        assert!(text.contains("arrived went 6 min ago"), "{text}");
        // Older than the server believes: it shows this person as offline,
        // which is the part a person would not otherwise work out.
        assert!(text.contains("offline"), "{text}");
    }

    #[test]
    fn a_watcher_that_stopped_is_called_out() {
        // The record of a watcher that was killed looks healthy forever - the
        // last pulse did arrive - so silence is read off the time it meant to
        // try next.
        let now = Utc::now();
        let lines = describe(true, Some(&accepted(now, 600)), now);
        let text = rendered(&lines).join("\n");
        assert!(text.contains("kasl watch"), "{text}");
        assert!(text.contains("offline"), "{text}");
    }

    #[test]
    fn a_punctual_watcher_is_not_called_silent() {
        let now = Utc::now();
        let mut record = accepted(now, 50);
        record.next_at = at(SILENCE_GRACE_SECONDS, now);
        assert_eq!(describe(true, Some(&record), now).len(), 1);
    }

    #[test]
    fn a_skewed_clock_is_named_in_its_direction() {
        let now = Utc::now();
        let mut record = accepted(now, 5);
        record.clock_skew_seconds = Some(45);
        assert!(rendered(&describe(true, Some(&record), now)).join("\n").contains("45 s ahead"));

        record.clock_skew_seconds = Some(-SKEW_WORTH_MENTIONING_SECONDS);
        assert!(rendered(&describe(true, Some(&record), now)).join("\n").contains("behind"));

        record.clock_skew_seconds = Some(SKEW_WORTH_MENTIONING_SECONDS - 1);
        assert_eq!(describe(true, Some(&record), now).len(), 1);
    }
}
