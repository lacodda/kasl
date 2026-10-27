//! Where inbox issues come from, and how kasl acts on them there.
//!
//! An inbox issue has one source: kasl's own Jira integration, or a plugin
//! that answers `list_issues`. Everything the inbox does by itself - keeping
//! the rows, snoozing, triage, rules, toasts - is kasl's and the same for
//! every source; a source only says what is assigned, and moves or comments
//! on an issue when asked.
//!
//! The row's `source` decides where an action goes. A key the inbox has never
//! seen goes to Jira, as it always did: Jira is the one source kasl can ask
//! about an issue it did not list.

use crate::api::jira::SessionMode;
use crate::db::inbox::{Inbox, InboxItem, JIRA_SOURCE};
use crate::db::inbox_statuses::InboxStatuses;
use crate::libs::jira_bridge::{self, Goal, MoveOptions, Moved};
use crate::libs::json;
use crate::libs::plugin::manifest::{self, Request};
use crate::libs::plugin::{self, Plugin};
use anyhow::{Result, anyhow, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

/// One issue as a plugin source lists it.
///
/// `key`, `summary` and `url` are what an inbox row cannot be without: what
/// it is called, what it says, and what opening it opens. The rest is
/// optional, and a rule or a filter that reads a field the source does not
/// give finds nothing there.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ListedIssue {
    pub key: String,
    pub summary: String,
    pub url: String,
    /// The source's own id, when it has one besides the key.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    /// Lower is more urgent, as Jira ranks its priorities (1 is Highest).
    #[serde(default)]
    pub priority_rank: Option<i32>,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default, rename = "type")]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    /// When the source last changed it, in its own words.
    #[serde(default)]
    pub updated: Option<String>,
}

/// What a source answered to `list_issues`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Listing {
    pub issues: Vec<ListedIssue>,
    /// The page at the source that lists these issues, for a summary toast to open.
    #[serde(default)]
    pub list_url: Option<String>,
}

/// The plugins that are inbox sources and set up, sorted by name.
pub fn sources() -> Result<Vec<Plugin>> {
    Ok(plugin::discover()?
        .into_iter()
        .filter(|plugin| plugin.ready() && plugin.manifest.as_ref().is_some_and(|m| m.is_source()))
        .collect())
}

/// Asks a source for the issues assigned now.
///
/// Read whole or refused whole. The answer decides which of the source's
/// rows are gone, so a reply kasl could only half read must not be taken for
/// a short list: an issue that failed to parse would be marked as having
/// left. Silence is refused for the same reason - an empty list is `[]`.
pub fn list_issues(plugin: &Plugin) -> Result<Listing> {
    let reply = plugin::invoke(plugin, json!({ "request": Request::ListIssues.as_str() }), manifest::allowance(None))?;
    read_listing(Value::Object(reply)).map_err(|why| anyhow!("the plugin `{}` answered list_issues {why}", plugin.name))
}

fn read_listing(reply: Value) -> std::result::Result<Listing, String> {
    if reply.get("issues").is_none() {
        return Err("without `issues`; a source with nothing assigned answers `\"issues\": []`".to_string());
    }
    let listing: Listing = serde_json::from_value(reply).map_err(|error| format!("with what kasl cannot read: {error}"))?;
    let mut seen = HashSet::new();
    for issue in &listing.issues {
        if issue.key.trim().is_empty() {
            return Err(format!("with an issue that has no key ({})", issue.summary));
        }
        if issue.url.trim().is_empty() {
            return Err(format!("with {} and no url to open it at", issue.key));
        }
        if !seen.insert(issue.key.as_str()) {
            return Err(format!("with {} twice", issue.key));
        }
    }
    Ok(listing)
}

/// The id a source's status is kept under: `gitlab:Opened`.
///
/// A plugin gives a status by name, and Jira's ids are numbers; prefixing the
/// source keeps two sources' "Open" apart and can never meet a Jira id.
pub fn status_id(source: &str, status: &str) -> String {
    format!("{source}:{status}")
}

/// How the messages name a source: "Jira", or the plugin's own name.
pub fn label(source: &str) -> String {
    if source == JIRA_SOURCE { "Jira".to_string() } else { source.to_string() }
}

/// The source the row of `key` belongs to; Jira for a key the inbox has not seen.
pub fn source_of(key: &str) -> Result<String> {
    Ok(Inbox::new()?
        .get_by_key(key)?
        .map(|item| item.source)
        .unwrap_or_else(|| JIRA_SOURCE.to_string()))
}

/// The plugin behind `source`, set up and answering `request`.
fn plugin_for(source: &str, request: Request) -> Result<Plugin> {
    let plugin = plugin::find(source)?;
    let manifest = plugin.ensure_ready()?;
    if !manifest.answers(request) {
        bail!(
            "the plugin `{source}` does not answer `{}`, so kasl cannot do that to its issues",
            request.as_str()
        );
    }
    Ok(plugin)
}

fn item(key: &str) -> Result<InboxItem> {
    Inbox::new()?.get_by_key(key)?.ok_or_else(|| anyhow!("{key} is not in the inbox"))
}

/// Moves `key` towards `goal` at its source.
pub async fn move_issue(key: &str, goal: Goal, options: &MoveOptions) -> Result<Moved> {
    let source = source_of(key)?;
    if source == JIRA_SOURCE {
        let mut jira = jira_bridge::client()?;
        return jira_bridge::move_issue(&mut jira, key, goal, options).await;
    }
    let call = transition_call(&item(key)?, goal, options, None);
    transition(&source, call).await
}

/// Comments on `key`, then closes it, at its source.
pub async fn close_issue(key: &str, comment: Option<&str>, options: &MoveOptions) -> Result<Moved> {
    let source = source_of(key)?;
    if source == JIRA_SOURCE {
        let mut jira = jira_bridge::client()?;
        return jira_bridge::close_issue(&mut jira, key, comment, options).await;
    }
    let comment = comment.map(str::trim).filter(|body| !body.is_empty());
    let call = transition_call(&item(key)?, Goal::Close, options, comment);
    transition(&source, call).await
}

/// Leaves a comment on `key` at its source.
pub async fn comment_issue(key: &str, text: &str, session: SessionMode) -> Result<()> {
    let source = source_of(key)?;
    if source == JIRA_SOURCE {
        let mut jira = jira_bridge::client()?;
        return jira.add_comment(key, text, session).await;
    }
    let plugin = plugin_for(&source, Request::Comment)?;
    let call = json!({
        "request": Request::Comment.as_str(),
        "subject": json::Issue::now(&item(key)?),
        "comment": text,
    });
    run(plugin, call).await.map(|_| ())
}

/// Whether the source of `key` can be asked to close it and to take a comment.
///
/// What the offer after finishing a task may put to the person: a source that
/// cannot do either is not offered, rather than offered and refused.
pub fn can_close_and_comment(key: &str) -> Result<(bool, bool)> {
    let source = source_of(key)?;
    if source == JIRA_SOURCE {
        let configured = crate::libs::config::Config::read()?.jira.is_some();
        return Ok((configured, configured));
    }
    let Ok(plugin) = plugin::find(&source) else {
        return Ok((false, false));
    };
    match plugin.ensure_ready() {
        Ok(manifest) => Ok((manifest.answers(Request::Transition), manifest.answers(Request::Comment))),
        Err(_) => Ok((false, false)),
    }
}

/// The `transition` request for one issue: where it goes, and what the person named.
fn transition_call(item: &InboxItem, goal: Goal, options: &MoveOptions, comment: Option<&str>) -> Value {
    json!({
        "request": Request::Transition.as_str(),
        "subject": json::Issue::now(item),
        "to": goal.verb(),
        "status": options.to,
        "resolution": options.resolution,
        "comment": comment,
    })
}

/// Sends a `transition` and keeps the status the source says the issue is in now.
async fn transition(source: &str, call: Value) -> Result<Moved> {
    let plugin = plugin_for(source, Request::Transition)?;
    let key = call["subject"]["key"].as_str().unwrap_or_default().to_string();
    let reply = run(plugin, call).await?;
    let moved = read_transition(&reply).map_err(|why| anyhow!("the plugin `{source}` answered transition {why}"))?;
    let status = match &moved {
        Moved::Moved { to } => to,
        Moved::Already { status } => status,
        _ => return Ok(moved),
    };
    let id = status_id(source, status);
    InboxStatuses::new()?.upsert(&id, status)?;
    Inbox::new()?.set_status(&key, &id)?;
    Ok(moved)
}

/// Reads a `transition` reply: the status the issue is in now, and whether
/// this request is what put it there.
fn read_transition(reply: &serde_json::Map<String, Value>) -> std::result::Result<Moved, String> {
    let status = reply
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .ok_or("without the `status` the issue is in now")?
        .to_string();
    match reply.get("moved").and_then(Value::as_bool) {
        Some(true) => Ok(Moved::Moved { to: status }),
        Some(false) => Ok(Moved::Already { status }),
        None => Err("without `moved`: whether this request moved it, or it was already there".to_string()),
    }
}

/// Runs one request off the async runtime's threads.
async fn run(plugin: Plugin, call: Value) -> Result<serde_json::Map<String, Value>> {
    tokio::task::spawn_blocking(move || plugin::invoke(&plugin, call, manifest::allowance(None)))
        .await
        .map_err(|error| anyhow!("the plugin call stopped: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(raw: Value) -> std::result::Result<Listing, String> {
        read_listing(raw)
    }

    #[test]
    fn a_listing_needs_its_issues_even_when_there_are_none() {
        assert!(listing(json!({})).unwrap_err().contains("without `issues`"));
        assert_eq!(listing(json!({"issues": []})).unwrap().issues, Vec::new());
    }

    #[test]
    fn an_issue_needs_a_key_a_summary_and_somewhere_to_open() {
        let without_url = json!({"issues": [{"key": "g#1", "summary": "s", "url": ""}]});
        let without_summary = json!({"issues": [{"key": "g#1", "url": "https://x"}]});
        let without_key = json!({"issues": [{"key": " ", "summary": "s", "url": "https://x"}]});

        assert!(listing(without_url).unwrap_err().contains("no url"));
        assert!(listing(without_summary).unwrap_err().contains("cannot read"));
        assert!(listing(without_key).unwrap_err().contains("no key"));
    }

    #[test]
    fn a_key_listed_twice_is_refused() {
        let twice = json!({"issues": [
            {"key": "g#1", "summary": "a", "url": "https://x/1"},
            {"key": "g#1", "summary": "b", "url": "https://x/1"}
        ]});

        assert!(listing(twice).unwrap_err().contains("g#1 twice"));
    }

    #[test]
    fn every_optional_field_is_read() {
        let full = json!({"list_url": "https://x/issues", "issues": [{
            "key": "g#1", "summary": "Fix it", "url": "https://x/1", "id": "901", "status": "Opened",
            "priority": "High", "priority_rank": 2, "score": 7.5, "type": "Bug", "project": "group/app",
            "updated": "2026-10-27T10:00:00Z", "labels": ["something kasl does not read"]
        }]});

        let read = listing(full).unwrap();

        assert_eq!(read.list_url.as_deref(), Some("https://x/issues"));
        let issue = &read.issues[0];
        assert_eq!(issue.issue_type.as_deref(), Some("Bug"));
        assert_eq!((issue.priority_rank, issue.score), (Some(2), Some(7.5)));
    }

    #[test]
    fn a_transition_reply_says_where_the_issue_is_and_whether_it_moved() {
        let reply = |value: Value| read_transition(value.as_object().unwrap());

        assert_eq!(reply(json!({"status": "Doing", "moved": true})), Ok(Moved::Moved { to: "Doing".into() }));
        assert_eq!(reply(json!({"status": "Doing", "moved": false})), Ok(Moved::Already { status: "Doing".into() }));
        assert!(reply(json!({"moved": true})).unwrap_err().contains("`status`"));
        assert!(reply(json!({"status": "Doing"})).unwrap_err().contains("`moved`"));
    }

    #[test]
    fn a_source_status_never_meets_a_jira_id() {
        assert_eq!(status_id("gitlab", "Opened"), "gitlab:Opened");
        assert_eq!(label(JIRA_SOURCE), "Jira");
        assert_eq!(label("gitlab"), "gitlab");
    }
}
