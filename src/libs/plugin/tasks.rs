//! `find_tasks`: what plugins find in a day for `kasl task find` to offer.
//!
//! A plugin that answers `find_tasks` is handed the day - as
//! `kasl report --json` prints it - and names tasks it finds there: the
//! commits pushed to a forge, the documents edited, the calls held. kasl
//! offers them beside its own unfinished tasks and Jira's resolved issues;
//! nothing is imported until the person picks it.

use super::manifest::{self, Request};
use super::{Plugin, discover, invoke};
use anyhow::{Result, anyhow};
use serde_json::{Map, Value, json};

/// One task a plugin found.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundTask {
    /// What the task would be called.
    pub name: String,
    /// A short reference shown beside the name: a commit's SHA, a page's title.
    pub reference: Option<String>,
}

/// What one plugin answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub tasks: Vec<FoundTask>,
    /// A line the plugin wants said - what it could not read, say.
    pub message: Option<String>,
}

/// The plugins to ask: set up, and answering `find_tasks`, by name.
pub fn finders() -> Result<Vec<Plugin>> {
    Ok(discover()?
        .into_iter()
        .filter(|plugin| plugin.ready() && plugin.manifest.as_ref().is_some_and(|m| m.answers(Request::FindTasks)))
        .collect())
}

/// Asks one plugin for the tasks of `day` (the document `kasl report --json` prints).
pub fn find(plugin: &Plugin, day: Value) -> Result<Finding> {
    let reply = invoke(
        plugin,
        json!({ "request": Request::FindTasks.as_str(), "subject": day }),
        manifest::allowance(None),
    )?;
    read_finding(&reply).map_err(|why| anyhow!("the plugin `{}` answered find_tasks {why}", plugin.name))
}

/// Reads a `find_tasks` reply.
///
/// `tasks` is required, as `issues` is for a listing: a plugin that found
/// nothing says `[]`, and silence is more likely a plugin that did not run
/// its request than a day without work. A task without a name is refused
/// rather than offered blank.
fn read_finding(reply: &Map<String, Value>) -> std::result::Result<Finding, String> {
    let listed = reply
        .get("tasks")
        .and_then(Value::as_array)
        .ok_or("without `tasks`; a plugin that found nothing answers `\"tasks\": []`")?;
    let mut tasks = Vec::with_capacity(listed.len());
    for task in listed {
        let name = task
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("with a task that has no name: {task}"))?;
        let reference = task
            .get("ref")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|reference| !reference.is_empty())
            .map(str::to_string);
        tasks.push(FoundTask {
            name: name.to_string(),
            reference,
        });
    }
    let message = reply
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(str::to_string);
    Ok(Finding { tasks, message })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(raw: Value) -> std::result::Result<Finding, String> {
        read_finding(raw.as_object().unwrap())
    }

    #[test]
    fn tasks_are_read_with_their_references() {
        let finding = reply(json!({"tasks": [
            {"name": " Fix the login ", "ref": "a1b2c3d"},
            {"name": "Write the notes", "ref": ""},
            {"name": "Call", "extra": true}
        ], "message": "1 push could not be read"}))
        .unwrap();

        assert_eq!(
            finding.tasks,
            vec![
                FoundTask {
                    name: "Fix the login".into(),
                    reference: Some("a1b2c3d".into())
                },
                FoundTask {
                    name: "Write the notes".into(),
                    reference: None
                },
                FoundTask {
                    name: "Call".into(),
                    reference: None
                },
            ]
        );
        assert_eq!(finding.message.as_deref(), Some("1 push could not be read"));
    }

    #[test]
    fn nothing_found_is_said_with_an_empty_list() {
        assert_eq!(reply(json!({"tasks": []})).unwrap().tasks, Vec::new());
        assert!(reply(json!({})).unwrap_err().contains("without `tasks`"));
        assert!(reply(json!({"tasks": "none"})).unwrap_err().contains("without `tasks`"));
    }

    #[test]
    fn a_task_without_a_name_is_refused() {
        assert!(reply(json!({"tasks": [{"name": "  "}]})).unwrap_err().contains("no name"));
        assert!(reply(json!({"tasks": [{"ref": "a1"}]})).unwrap_err().contains("no name"));
    }
}
