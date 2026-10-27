//! What a plugin says about itself: `kasl-plugin-NAME --manifest`.
//!
//! The manifest is the whole of what kasl knows about a plugin before it runs
//! one of its calls, so everything kasl decides about a plugin - which events
//! it hears, whether it is an inbox source, what it needs to be set up - is
//! read from here. Every list is closed: an event, a request or a target kasl
//! does not know makes the manifest unreadable, and the plugin is listed with
//! that reason rather than half-used.

use serde::{Deserialize, Serialize};

/// The protocol version kasl speaks. A plugin declaring anything else is
/// listed but refused, with the mismatch shown - silently ignoring it would
/// look like the plugin was not installed.
pub const PROTOCOL_VERSION: u32 = 1;

/// The name a plugin executable must start with to be discovered.
pub const PREFIX: &str = "kasl-plugin-";

/// What a call may take when the manifest names no allowance: two minutes,
/// generous for a lookup, a mail or a request to a tracker.
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 120;

/// The longest allowance a manifest may claim: an hour. A plugin is someone
/// else's program, and one that says it needs a day has made a typo.
pub const LONGEST_TIMEOUT_SECONDS: u64 = 3600;

/// What a plugin prints for `--manifest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub protocol_version: u32,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What a person can run by hand: `kasl plugin run NAME KEY`.
    #[serde(default)]
    pub commands: Vec<Command>,
    /// The moments kasl tells this plugin about.
    #[serde(default)]
    pub events: Vec<Event>,
    /// What the plugin answers as an inbox source. One that answers
    /// `list_issues` is a source; `transition` and `comment` are what it can
    /// do to an issue it listed.
    #[serde(default)]
    pub requests: Vec<Request>,
    /// What it needs to know: a URL, an account, a token.
    #[serde(default)]
    pub settings: Vec<Setting>,
}

/// One thing a person can run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What the command is run on.
    pub target: Target,
    /// Seconds the command may run before kasl stops it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// What a command is run on, and so what it is handed as its subject.
///
/// Deliberately few: each is a document kasl promises to keep handing over in
/// the same shape, so one added is a commitment and none can quietly go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// A day, as `kasl report --json` prints it. `--date` picks it; today by default.
    Day,
    /// An inbox issue, as `kasl inbox show --json` prints it. The key picks it.
    Issue,
}

impl Target {
    pub fn as_str(self) -> &'static str {
        match self {
            Target::Day => "day",
            Target::Issue => "issue",
        }
    }
}

/// A moment kasl tells plugins about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// `kasl report --send` found the day ready to go out: its tasks are
    /// there and its productivity clears the minimum. The subject is the day.
    ReportReady,
    /// A day's end was written: by `kasl end`, or by the send that closes the
    /// day it sends. The subject is the day, its end included.
    WorkdayEnd,
    /// An issue arrived in the inbox and is not hidden by a rule. The subject
    /// is the issue.
    NewInboxIssue,
}

impl Event {
    /// Every event, in the order the docs list them.
    pub const ALL: [Event; 3] = [Event::ReportReady, Event::WorkdayEnd, Event::NewInboxIssue];

    pub fn as_str(self) -> &'static str {
        match self {
            Event::ReportReady => "report_ready",
            Event::WorkdayEnd => "workday_end",
            Event::NewInboxIssue => "new_inbox_issue",
        }
    }
}

/// What kasl asks of an inbox source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    /// The issues assigned to the person, whole, every poll.
    ListIssues,
    /// Move an issue it listed: start it, or close it.
    Transition,
    /// Leave a comment on an issue it listed.
    Comment,
}

impl Request {
    pub fn as_str(self) -> &'static str {
        match self {
            Request::ListIssues => "list_issues",
            Request::Transition => "transition",
            Request::Comment => "comment",
        }
    }
}

/// One thing a plugin needs to be told.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Kept in the OS keyring rather than in the config file, and typed at a
    /// prompt that does not echo.
    #[serde(default)]
    pub secret: bool,
    /// Without it the plugin is not set up: kasl does not call it for events
    /// or polls, and `kasl plugin run` says what is missing.
    #[serde(default)]
    pub required: bool,
}

impl Manifest {
    /// Why this manifest cannot be used, when it parses but does not hold together.
    ///
    /// The protocol version first: a manifest from another protocol may mean
    /// different things by the same words, so nothing else in it is judged.
    pub fn problem(&self) -> Option<String> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Some(format!("it speaks protocol {} and this kasl speaks {PROTOCOL_VERSION}", self.protocol_version));
        }
        if !self.requests.contains(&Request::ListIssues)
            && let Some(request) = self.requests.first()
        {
            return Some(format!(
                "it answers `{}` but not `list_issues`: an inbox source lists its issues first",
                request.as_str()
            ));
        }
        if let Some(key) = first_repeated(self.commands.iter().map(|c| c.key.as_str())) {
            return Some(format!("it declares the command `{key}` twice"));
        }
        if let Some(key) = first_repeated(self.settings.iter().map(|s| s.key.as_str())) {
            return Some(format!("it declares the setting `{key}` twice"));
        }
        if let Some(command) = self.commands.iter().find(|c| c.key.trim().is_empty()) {
            return Some(format!("its command `{}` has no key", command.label));
        }
        None
    }

    /// Whether this plugin is an inbox source.
    pub fn is_source(&self) -> bool {
        self.requests.contains(&Request::ListIssues)
    }

    /// Whether it answers `request`.
    pub fn answers(&self, request: Request) -> bool {
        self.requests.contains(&request)
    }

    /// Whether it hears `event`.
    pub fn hears(&self, event: Event) -> bool {
        self.events.contains(&event)
    }

    /// The command with this key.
    pub fn command(&self, key: &str) -> Option<&Command> {
        self.commands.iter().find(|c| c.key == key)
    }
}

impl Command {
    /// How long this command may run.
    pub fn allowance(&self) -> std::time::Duration {
        allowance(self.timeout)
    }
}

/// A manifest's claim turned into a time limit: absent or zero is the
/// default, and nothing is longer than an hour.
pub fn allowance(claimed: Option<u64>) -> std::time::Duration {
    let seconds = claimed
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
        .min(LONGEST_TIMEOUT_SECONDS);
    std::time::Duration::from_secs(seconds)
}

fn first_repeated<'a>(mut keys: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = std::collections::HashSet::new();
    keys.find(|key| !seen.insert(*key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(raw: &str) -> Manifest {
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn a_manifest_parses_with_only_its_required_fields() {
        let manifest = read(r#"{"protocol_version":1,"name":"demo","version":"0.1.0"}"#);

        assert!(manifest.commands.is_empty() && manifest.events.is_empty() && manifest.requests.is_empty());
        assert_eq!(manifest.problem(), None);
        assert!(!manifest.is_source());
    }

    #[test]
    fn events_requests_and_targets_are_read_by_name() {
        let manifest = read(
            r#"{"protocol_version":1,"name":"demo","version":"0.1.0",
                "commands":[{"key":"mail","label":"Mail the day","target":"day","timeout":30}],
                "events":["report_ready","workday_end","new_inbox_issue"],
                "requests":["list_issues","transition","comment"],
                "settings":[{"key":"token","label":"Token","secret":true,"required":true}]}"#,
        );

        assert_eq!(manifest.commands[0].target, Target::Day);
        assert_eq!(manifest.commands[0].allowance().as_secs(), 30);
        assert_eq!(manifest.events, Event::ALL.to_vec());
        assert!(manifest.is_source() && manifest.answers(Request::Comment));
        assert!(manifest.settings[0].secret && manifest.settings[0].required);
    }

    #[test]
    fn a_name_kasl_does_not_know_is_refused_rather_than_guessed() {
        let unknown_event = r#"{"protocol_version":1,"name":"d","version":"1","events":["lunch"]}"#;
        let unknown_request = r#"{"protocol_version":1,"name":"d","version":"1","requests":["list-issues"]}"#;
        let unknown_target = r#"{"protocol_version":1,"name":"d","version":"1","commands":[{"key":"x","label":"X","target":"week"}]}"#;

        for raw in [unknown_event, unknown_request, unknown_target] {
            assert!(serde_json::from_str::<Manifest>(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn another_protocol_is_named_as_the_problem() {
        let manifest = read(r#"{"protocol_version":2,"name":"d","version":"1"}"#);

        assert_eq!(manifest.problem().as_deref(), Some("it speaks protocol 2 and this kasl speaks 1"));
    }

    #[test]
    fn a_source_has_to_list_before_it_can_move() {
        let manifest = read(r#"{"protocol_version":1,"name":"d","version":"1","requests":["transition"]}"#);

        assert!(manifest.problem().unwrap().contains("not `list_issues`"));
        assert!(!manifest.is_source());
    }

    #[test]
    fn a_key_declared_twice_is_named() {
        let commands = read(
            r#"{"protocol_version":1,"name":"d","version":"1",
                "commands":[{"key":"a","label":"A","target":"day"},{"key":"a","label":"B","target":"issue"}]}"#,
        );
        let settings = read(
            r#"{"protocol_version":1,"name":"d","version":"1",
                "settings":[{"key":"url","label":"URL"},{"key":"url","label":"Again"}]}"#,
        );

        assert!(commands.problem().unwrap().contains("command `a` twice"));
        assert!(settings.problem().unwrap().contains("setting `url` twice"));
    }

    #[test]
    fn an_allowance_is_bounded_both_ways() {
        assert_eq!(allowance(None).as_secs(), DEFAULT_TIMEOUT_SECONDS);
        assert_eq!(allowance(Some(0)).as_secs(), DEFAULT_TIMEOUT_SECONDS);
        assert_eq!(allowance(Some(86_400)).as_secs(), LONGEST_TIMEOUT_SECONDS);
    }
}
