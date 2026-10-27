//! The two things kasl writes to Jira: an issue's place in its workflow, and a comment.
//!
//! Everything else about Jira is read: the inbox polls it, `task find` asks it
//! what was resolved today. This module is where kasl acts on it instead, and
//! it is kept to two moves - start and close - because those are the two
//! moments a local task and its issue meet: taking the issue, and finishing
//! the work.
//!
//! ## Moves by category, never by name
//!
//! A move is a transition whose target status falls into one of Jira's fixed
//! categories: `indeterminate` (in progress) to start, `done` to close. Status
//! and transition names are per-workflow and localized - "In Progress", "В
//! работе", "Start Review" - while the three categories are the same on every
//! Jira. The completed-issue search learned this the hard way: an English
//! "Done" in its JQL failed the whole query on a Russian Jira.
//!
//! When more than one transition leads into the target category, kasl does
//! not pick. On a terminal it asks; unattended (a toast button, a script) it
//! does nothing and says how to settle it, because moving a shared issue to
//! the wrong status is worse than not moving it - the first is a mistake
//! someone else sees, the second is a step still to take.
//!
//! The commands speak in these two moves rather than in transition ids, so a
//! source other than Jira can answer the same questions later.

use crate::api::jira::{Jira, JiraStatus, JiraTransition, SessionMode, StatusCategory};
use crate::db::inbox::{Inbox, JIRA_SOURCE, LeftReason};
use crate::db::inbox_statuses::InboxStatuses;
use crate::libs::config::Config;
use crate::libs::inbox_source;
use crate::libs::messages::Message;
use crate::libs::prompt::is_interactive;
use crate::{msg_info, msg_success, msg_warning};
use anyhow::{Result, bail};
use dialoguer::{Input, Select, theme::ColorfulTheme};

/// Where a move takes an issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    /// Into the in-progress category.
    Start,
    /// Into the done category.
    Close,
}

impl Goal {
    /// The category a transition must lead into to count as this move.
    pub fn category(self) -> StatusCategory {
        match self {
            Goal::Start => StatusCategory::InProgress,
            Goal::Close => StatusCategory::Done,
        }
    }

    /// The verb the messages use.
    pub fn verb(self) -> &'static str {
        match self {
            Goal::Start => "start",
            Goal::Close => "close",
        }
    }
}

/// How a move may proceed: whom it may ask, and what the caller already chose.
#[derive(Debug, Clone)]
pub struct MoveOptions {
    /// Whether signing in may prompt for a password.
    pub session: SessionMode,
    /// Target status or transition name, for when Jira offers several ways.
    pub to: Option<String>,
    /// Resolution name, for a close whose screen asks for one.
    pub resolution: Option<String>,
}

impl MoveOptions {
    /// A move with nobody to ask: a toast button, the watcher.
    pub fn unattended() -> Self {
        Self {
            session: SessionMode::Unattended,
            to: None,
            resolution: None,
        }
    }

    /// A move from the command line, which may sign in and ask on a terminal.
    pub fn from_cli(to: Option<String>, resolution: Option<String>) -> Self {
        Self {
            session: SessionMode::Interactive,
            to,
            resolution,
        }
    }

    /// Whether a choice can be put to a person.
    ///
    /// Both have to hold: an interactive session in a script still has
    /// nobody at the keyboard, and a terminal under the watcher is not one.
    fn can_ask(&self) -> bool {
        self.session == SessionMode::Interactive && is_interactive()
    }
}

/// What a move came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Moved {
    /// The issue is now in this status.
    Moved { to: String },
    /// The issue was already there; the status it is in.
    Already { status: String },
    /// No transition from the current status leads where the move goes.
    NoWay { status: String },
    /// Several transitions do, and nobody was there to choose.
    Ambiguous { options: Vec<String> },
    /// Jira does not know the issue, or will not show it to this account.
    Missing,
}

impl Moved {
    /// Whether the issue ended where the move meant it to be.
    pub fn arrived(&self) -> bool {
        matches!(self, Moved::Moved { .. } | Moved::Already { .. })
    }

    /// One line saying what happened, with the command that settles a move
    /// that did not happen.
    ///
    /// The issue's source is named, read from its row: a GitLab issue is now
    /// Doing in gitlab, not in Jira. The three answers only Jira gives -
    /// no way, several ways, no such issue - name Jira.
    pub fn sentence(&self, key: &str, goal: Goal) -> String {
        let verb = goal.verb();
        let at = || inbox_source::label(&inbox_source::source_of(key).unwrap_or_else(|_| JIRA_SOURCE.to_string()));
        match self {
            Moved::Moved { to } => format!("{key} is now {to} in {}", at()),
            Moved::Already { status } => format!("{key} is already {status} in {}", at()),
            Moved::NoWay { status } => format!("Jira offers no way to {verb} {key} from {status}"),
            Moved::Ambiguous { options } => format!(
                "Jira offers {} ways to {verb} {key} ({}); run `kasl inbox {verb} {key}` on a terminal, or pass --to",
                options.len(),
                options.join(", ")
            ),
            Moved::Missing => format!("Jira does not know {key}, or does not show it to you"),
        }
    }
}

/// The transitions a move could take, decided before anything is sent.
#[derive(Debug, PartialEq, Eq)]
pub enum Route<'a> {
    /// The issue is already where the move goes.
    Already,
    /// Nothing leads there from the current status.
    NoWay,
    /// Exactly one transition does.
    One(&'a JiraTransition),
    /// More than one does; someone has to choose.
    Several(Vec<&'a JiraTransition>),
}

/// Whether an issue in `status` (of `category`) is already where the move goes.
///
/// With `to` set, "there" means that very status: an issue In Review that is
/// asked to go to In Progress is not there yet, though both are in progress.
pub fn already_there(status: &str, category: StatusCategory, goal: Goal, to: Option<&str>) -> bool {
    match to {
        Some(name) => status.eq_ignore_ascii_case(name),
        None => category == goal.category(),
    }
}

/// Picks the transitions that carry an issue in `current_status` to `goal`.
///
/// `to` narrows by name, matching either the target status ("Closed") or the
/// transition itself ("Resolve Issue"), case-insensitively - whichever word
/// the person knows from their Jira screen.
pub fn route<'a>(current_status: &str, current: StatusCategory, transitions: &'a [JiraTransition], goal: Goal, to: Option<&str>) -> Route<'a> {
    if already_there(current_status, current, goal, to) {
        return Route::Already;
    }

    let candidates: Vec<&JiraTransition> = transitions
        .iter()
        .filter(|t| t.to.category() == goal.category())
        .filter(|t| to.is_none_or(|name| t.to.name.eq_ignore_ascii_case(name) || t.name.eq_ignore_ascii_case(name)))
        .collect();

    match candidates.len() {
        0 => Route::NoWay,
        1 => Route::One(candidates[0]),
        _ => Route::Several(candidates),
    }
}

/// How a transition is named in a choice: the transition, and where it leads
/// when the two differ.
fn transition_label(transition: &JiraTransition) -> String {
    if transition.name.eq_ignore_ascii_case(&transition.to.name) {
        transition.name.clone()
    } else {
        format!("{} → {}", transition.name, transition.to.name)
    }
}

/// A move decided but not yet sent: which transition, with which resolution.
struct Plan {
    transition_id: String,
    resolution_id: Option<String>,
    to: JiraStatus,
}

/// What deciding a move came to: a move to send, or an outcome already.
enum Planned {
    Go(Plan),
    Settled(Moved),
}

/// Decides how `key` moves towards `goal`, asking whatever has to be asked,
/// and sends nothing.
///
/// Everything that can stop a move is settled here - the issue missing, no
/// way or several, a resolution nobody named - so that a caller with a second
/// write to make (a comment) can make it only once the move is certain.
async fn plan_move(jira: &mut Jira, key: &str, goal: Goal, options: &MoveOptions) -> Result<Planned> {
    let Some(state) = jira.issue_state(key, options.session).await? else {
        return Ok(Planned::Settled(Moved::Missing));
    };

    // Settled before asking Jira for its transitions: an issue already there
    // needs no second request to say so.
    if already_there(&state.status.name, state.status.category(), goal, options.to.as_deref()) {
        return Ok(Planned::Settled(Moved::Already { status: state.status.name }));
    }
    let transitions = jira.transitions(key, options.session).await?;

    let transition = match route(&state.status.name, state.status.category(), &transitions, goal, options.to.as_deref()) {
        Route::Already => return Ok(Planned::Settled(Moved::Already { status: state.status.name })),
        Route::NoWay => return Ok(Planned::Settled(Moved::NoWay { status: state.status.name })),
        Route::One(transition) => transition,
        Route::Several(options_offered) => {
            let labels: Vec<String> = options_offered.iter().map(|t| transition_label(t)).collect();
            if !options.can_ask() {
                return Ok(Planned::Settled(Moved::Ambiguous { options: labels }));
            }
            let prompt = format!("Jira offers several ways to {} {key}", goal.verb());
            let index = Select::with_theme(&ColorfulTheme::default())
                .with_prompt(prompt)
                .items(&labels)
                .default(0)
                .interact()?;
            options_offered[index]
        }
    };

    let resolution_id = match goal {
        Goal::Close => resolution_for(key, transition, options)?,
        Goal::Start => None,
    };
    Ok(Planned::Go(Plan {
        transition_id: transition.id.clone(),
        resolution_id,
        to: transition.to.clone(),
    }))
}

/// Sends a decided move, and writes where the issue went into the inbox.
///
/// The local row is brought up to date at once rather than at the next poll.
/// Otherwise the inbox shows the old status until then - and the poll, seeing
/// the status change, raises a "status→In Progress" toast about the move the
/// person has just made themselves.
async fn send_move(jira: &mut Jira, key: &str, plan: Plan, options: &MoveOptions) -> Result<Moved> {
    jira.transition(key, &plan.transition_id, plan.resolution_id.as_deref(), options.session)
        .await?;
    if let Err(e) = remember_status(key, &plan.to) {
        tracing::warn!("{} moved in Jira, but the inbox row was not updated: {}", key, e);
    }
    Ok(Moved::Moved { to: plan.to.name })
}

/// Records `status` as the issue's current one in the local inbox.
fn remember_status(key: &str, status: &JiraStatus) -> Result<()> {
    if status.id.is_empty() {
        return Ok(());
    }
    InboxStatuses::new()?.upsert(&status.id, &status.name)?;
    Inbox::new()?.set_status(key, &status.id)?;
    Ok(())
}

/// Moves `key` along its workflow towards `goal`.
pub async fn move_issue(jira: &mut Jira, key: &str, goal: Goal, options: &MoveOptions) -> Result<Moved> {
    match plan_move(jira, key, goal, options).await? {
        Planned::Go(plan) => send_move(jira, key, plan, options).await,
        Planned::Settled(moved) => Ok(moved),
    }
}

/// The resolution a close sends, when its screen asks for one.
///
/// Jira refuses a close whose screen requires a resolution and gets none, and
/// there is no default kasl could honestly assume: "Fixed", "Done" and "Won't
/// Do" all close an issue and mean different things on the team's reports.
/// One allowed value is no choice at all; several are asked about on a
/// terminal and named by `--resolution` everywhere else.
fn resolution_for(key: &str, transition: &JiraTransition, options: &MoveOptions) -> Result<Option<String>> {
    let Some(field) = transition.fields.get("resolution").filter(|f| f.required) else {
        return Ok(None);
    };
    let allowed = &field.allowed_values;
    if let Some(wanted) = &options.resolution {
        return match allowed.iter().find(|v| v.name.eq_ignore_ascii_case(wanted)) {
            Some(value) => Ok(Some(value.id.clone())),
            None => bail!(
                "Jira does not offer the resolution '{wanted}' for {key}; it offers {}",
                names(allowed.iter().map(|v| v.name.as_str()))
            ),
        };
    }
    match allowed.len() {
        0 => Ok(None),
        1 => Ok(Some(allowed[0].id.clone())),
        _ if options.can_ask() => {
            let labels: Vec<&str> = allowed.iter().map(|v| v.name.as_str()).collect();
            let index = Select::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Resolution for {key}"))
                .items(&labels)
                .default(0)
                .interact()?;
            Ok(Some(allowed[index].id.clone()))
        }
        _ => bail!(
            "closing {key} needs a resolution - {}; pass --resolution NAME",
            names(allowed.iter().map(|v| v.name.as_str()))
        ),
    }
}

fn names<'a>(values: impl Iterator<Item = &'a str>) -> String {
    values.collect::<Vec<_>>().join(", ")
}

/// Comments on `key`, then closes it.
///
/// The move is decided first and the comment sent only when the close is
/// certain - or already done, since then the note is still what the person
/// asked to leave. A comment sent ahead of a close that then turned out to
/// need `--to` would be sent again by the retry: one note, twice, on a shared
/// issue.
///
/// Sent, the comment goes before the transition. A closed issue is often not
/// editable - `jira.issue.editable = false` on the final status refuses
/// comments too - so closing first could leave the note with nowhere to go. A
/// comment that fails stops the close: the person asked for both, and an issue
/// closed without the explanation they wrote is the half they did not want.
pub async fn close_issue(jira: &mut Jira, key: &str, comment: Option<&str>, options: &MoveOptions) -> Result<Moved> {
    let planned = plan_move(jira, key, Goal::Close, options).await?;
    let worth_a_comment = matches!(planned, Planned::Go(_) | Planned::Settled(Moved::Already { .. }));
    if worth_a_comment && let Some(body) = comment.map(str::trim).filter(|b| !b.is_empty()) {
        jira.add_comment(key, body, options.session).await?;
        msg_success!(Message::JiraCommented(key.to_string()));
    }
    match planned {
        Planned::Go(plan) => send_move(jira, key, plan, options).await,
        Planned::Settled(moved) => Ok(moved),
    }
}

/// Builds a Jira client from the config, or says why there is none.
pub fn client() -> Result<Jira> {
    let config = Config::read()?;
    match config.jira {
        Some(jira) => Ok(Jira::new(&jira)),
        None => bail!("Jira is not configured; run `kasl setup` and choose Jira"),
    }
}

/// What the offer after finishing a linked task can do in Jira.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Offer {
    Leave,
    Close,
    Comment,
}

/// Offers to close or comment on the issue a task came from, once the task is done.
///
/// Asked only on a terminal, only of what the issue's source can do - Jira
/// when it is configured, a plugin when it answers `transition` or
/// `comment` - and not when the inbox already knows the issue was resolved:
/// then the work is done on both sides and the question would be noise. Leaving it is the default: Enter on
/// a question about a shared tracker should change nothing in it.
///
/// Never fails the caller. The task is already saved when this runs; a Jira
/// that does not answer is reported and the local edit stands.
pub async fn offer_after_done(key: &str) {
    if !is_interactive() {
        return;
    }
    if let Err(e) = offer_after_done_inner(key).await {
        msg_warning!(Message::JiraWriteFailed(key.to_string(), e.to_string()));
    }
}

async fn offer_after_done_inner(key: &str) -> Result<()> {
    let (can_close, can_comment) = inbox_source::can_close_and_comment(key)?;
    if !can_close && !can_comment {
        return Ok(());
    }
    if let Some(item) = Inbox::new()?.get_by_key(key)?
        && item.left_reason == Some(LeftReason::Resolved)
    {
        return Ok(());
    }

    let mut choices = vec![Offer::Leave];
    let mut labels = vec![format!("Leave {key} as it is")];
    if can_close {
        choices.push(Offer::Close);
        labels.push(format!("Close {key}"));
    }
    if can_comment {
        choices.push(Offer::Comment);
        labels.push(format!("Comment on {key}"));
    }
    let index = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptJiraAfterDone(key.to_string()).to_string())
        .items(&labels)
        .default(0)
        .interact()?;

    match choices[index] {
        Offer::Leave => Ok(()),
        Offer::Close => {
            let comment: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::PromptJiraCloseComment.to_string())
                .allow_empty(true)
                .interact_text()?;
            let options = MoveOptions::from_cli(None, None);
            let moved = inbox_source::close_issue(key, Some(&comment), &options).await?;
            report_move(key, Goal::Close, &moved);
            Ok(())
        }
        Offer::Comment => {
            let comment: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(Message::PromptJiraComment.to_string())
                .interact_text()?;
            inbox_source::comment_issue(key, comment.trim(), SessionMode::Interactive).await?;
            msg_success!(Message::JiraCommented(key.to_string()));
            Ok(())
        }
    }
}

/// Prints what a move came to: success for arriving, a warning for the rest.
pub fn report_move(key: &str, goal: Goal, moved: &Moved) {
    match moved {
        Moved::Moved { .. } => msg_success!(Message::JiraMoved(moved.sentence(key, goal))),
        Moved::Already { .. } => msg_info!(Message::JiraMoved(moved.sentence(key, goal))),
        _ => msg_warning!(Message::JiraMoved(moved.sentence(key, goal))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::jira::{JiraCategory, JiraStatus};
    use std::collections::HashMap;

    fn transition(id: &str, name: &str, to: &str, category: &str) -> JiraTransition {
        JiraTransition {
            id: id.to_string(),
            name: name.to_string(),
            to: JiraStatus {
                id: String::new(),
                name: to.to_string(),
                category: Some(JiraCategory { key: category.to_string() }),
            },
            fields: HashMap::new(),
        }
    }

    fn workflow() -> Vec<JiraTransition> {
        vec![
            transition("11", "Start Progress", "In Progress", "indeterminate"),
            transition("21", "Resolve Issue", "Resolved", "done"),
            transition("31", "Close Issue", "Closed", "done"),
        ]
    }

    #[test]
    fn a_move_is_chosen_by_category_not_by_name() {
        // A localized workflow: no English word in sight, and the move still
        // finds the one transition into the in-progress category.
        let localized = vec![
            transition("5", "Взять в работу", "В работе", "indeterminate"),
            transition("6", "Отложить", "Открыт", "new"),
        ];
        match route("Открыт", StatusCategory::ToDo, &localized, Goal::Start, None) {
            Route::One(t) => assert_eq!(t.id, "5"),
            other => panic!("expected one way to start, got {other:?}"),
        }
    }

    #[test]
    fn an_issue_already_in_the_category_is_not_moved() {
        assert_eq!(route("In Review", StatusCategory::InProgress, &workflow(), Goal::Start, None), Route::Already);
        assert_eq!(route("Closed", StatusCategory::Done, &workflow(), Goal::Close, None), Route::Already);
    }

    #[test]
    fn several_ways_are_a_choice_not_a_guess() {
        match route("In Progress", StatusCategory::InProgress, &workflow(), Goal::Close, None) {
            Route::Several(ts) => assert_eq!(ts.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["21", "31"]),
            other => panic!("two ways to close must be offered, not picked: {other:?}"),
        }
    }

    #[test]
    fn to_settles_the_choice_by_status_or_transition_name() {
        match route("In Progress", StatusCategory::InProgress, &workflow(), Goal::Close, Some("closed")) {
            Route::One(t) => assert_eq!(t.id, "31"),
            other => panic!("--to Closed names the status: {other:?}"),
        }
        match route("In Progress", StatusCategory::InProgress, &workflow(), Goal::Close, Some("Resolve Issue")) {
            Route::One(t) => assert_eq!(t.id, "21"),
            other => panic!("--to may name the transition too: {other:?}"),
        }
        // A name outside the move's category does not smuggle in another move.
        assert_eq!(
            route("In Progress", StatusCategory::InProgress, &workflow(), Goal::Close, Some("Start Progress")),
            Route::NoWay
        );
    }

    #[test]
    fn to_means_that_very_status() {
        // In Review is in progress, but the person asked for In Progress.
        let flow = vec![transition("12", "Back to work", "In Progress", "indeterminate")];
        match route("In Review", StatusCategory::InProgress, &flow, Goal::Start, Some("In Progress")) {
            Route::One(t) => assert_eq!(t.id, "12"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            route("In Progress", StatusCategory::InProgress, &flow, Goal::Start, Some("in progress")),
            Route::Already
        );
    }

    #[test]
    fn no_transition_into_the_category_is_no_way() {
        let flow = vec![transition("6", "Reopen", "Open", "new")];
        assert_eq!(route("Resolved", StatusCategory::Done, &flow, Goal::Start, None), Route::NoWay);
    }

    #[test]
    fn an_ambiguous_move_names_its_way_out() {
        let text = Moved::Ambiguous {
            options: vec!["Resolve Issue → Resolved".into(), "Close Issue → Closed".into()],
        }
        .sentence("KA-1", Goal::Close);
        assert!(text.contains("kasl inbox close KA-1") && text.contains("--to"), "{text}");
    }
}
