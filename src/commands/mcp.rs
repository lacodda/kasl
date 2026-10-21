//! kasl's records, served to a coding assistant over MCP - reading only.
//!
//! `kasl mcp` speaks the transport MCP defines for a local server, the way
//! the line's other tools serve theirs: JSON-RPC 2.0, one message per line,
//! requests on stdin and responses on stdout. Nothing else may reach stdout -
//! a stray line is a protocol error to the client - so the run prints the way
//! a `--json` run does, every message on stderr.
//!
//! Every tool answers with the document the matching `--json` command prints,
//! built by the same function ([`DayReport::read`], [`Summary`], the inbox's
//! `list_document`, the server's `status_document`): a field added for one
//! reader is there for every reader, and the contract in [`crate::libs::json`]
//! holds here unchanged. See ADR 0008.
//!
//! Reading only. Writing - a task, a pause, closing the day - comes later, and
//! is not here in a disabled form either: a tool an assistant can see is a
//! tool it will try.
//!
//! The protocol is spoken directly rather than through an SDK: the stdio half
//! of MCP is a handful of methods. The loop is async because two of the reads
//! reach a server - the connection's status, the company's calendar.

use crate::commands::{inbox, pauses::DayPauses, server};
use crate::db::{jira_inbox::Include, tasks::Tasks};
use crate::libs::{json, messages::Message, output, report::DayReport, summary::Summary, task::TaskFilter};
use crate::msg_error_anyhow;
use anyhow::{Result, bail};
use chrono::{Local, NaiveDate};
use clap::ValueEnum;
use serde_json::{Map, Value, json};
use std::io::{IsTerminal, Write};
use tokio::io::AsyncBufReadExt;

/// The protocol version this server speaks, as rigger's does. A client that
/// asks for another is answered with this one: the specification has the
/// server name what it supports and the client decide whether it can live
/// with it.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// JSON-RPC error codes used here, from the specification.
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Serves the records on stdin/stdout until the client closes the stream.
pub async fn cmd() -> Result<()> {
    // A person who typed `kasl mcp` sees nothing and waits; a line on stderr
    // says why, and leaves the protocol's stream alone.
    if std::io::stdin().is_terminal() {
        eprintln!(
            "kasl mcp serves kasl's records over MCP on stdin and stdout, for a coding assistant to start: \
             `claude mcp add -s user kasl -- kasl mcp`. Ctrl+C ends it."
        );
    }

    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut line = Vec::new();
    loop {
        line.clear();
        if stdin.read_until(b'\n', &mut line).await? == 0 {
            return Ok(());
        }
        // A line that is not JSON - or not even text - has no id to answer
        // against, so there is nobody to answer; saying so on stderr is all a
        // server can do without corrupting the stream, and the session goes on.
        let request: Value = match std::str::from_utf8(&line).map_err(|err| err.to_string()).and_then(|text| {
            let text = text.trim();
            if text.is_empty() {
                return Ok(Value::Null);
            }
            serde_json::from_str(text).map_err(|err| err.to_string())
        }) {
            Ok(Value::Null) => continue,
            Ok(value) => value,
            Err(err) => {
                eprintln!("kasl mcp: ignoring a line that is not JSON: {err}");
                continue;
            }
        };
        let Some(response) = handle(&request).await else {
            // A notification: the specification forbids a response.
            continue;
        };
        if !send(&response)? {
            return Ok(());
        }
    }
}

/// Writes one response line; `false` when the client has gone away, which
/// ends the session quietly rather than as a failure.
fn send(response: &Value) -> Result<bool> {
    let mut stdout = std::io::stdout().lock();
    match writeln!(stdout, "{response}").and_then(|()| stdout.flush()) {
        Ok(()) => Ok(true),
        Err(error) if output::is_a_closed_pipe_error(&error) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Answers one message, or `None` when it is a notification.
async fn handle(request: &Value) -> Option<Value> {
    // Notifications carry no id and expect nothing back.
    let id = request.get("id").cloned()?;
    let method = request.get("method").and_then(Value::as_str).unwrap_or_default();
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = match method {
        "initialize" => Ok(initialize()),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(&params).await,
        other => Err(Failure {
            code: METHOD_NOT_FOUND,
            message: format!("kasl does not serve `{other}`"),
        }),
    };

    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(failure) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": failure.code, "message": failure.message },
        }),
    })
}

/// A protocol-level failure: the request itself was malformed. A tool that
/// runs and fails is not one of these - it answers with its reason, so the
/// assistant reads what went wrong instead of the client swallowing it.
struct Failure {
    code: i64,
    message: String,
}

fn initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "kasl", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "The work record kasl keeps on this machine, read-only: workdays and the \
    intervals between their pauses, tasks, the Jira inbox, the connection to the team's kasl-server. \
    `day` is where a day stands - today unless you give a `date` - with its intervals, work time, \
    productivity, tasks and the inbox at a glance; `week` and `month` total days the same way; `pauses` \
    lists a day's absences; `tasks` lists tasks by day, tag or id. `inbox` lists the assigned Jira \
    issues as the local table holds them, cut and ordered like `kasl inbox list`, and `issue` reads \
    one whole. `server_status` asks the team server whether this machine's connection works; \
    `server_queue` lists the days still owed to it. Every tool answers with the JSON document the \
    matching `kasl ... --json` command prints: moments are local time YYYY-MM-DDTHH:MM:SS, \
    durations whole seconds in fields ending in _seconds, percentages 0-100. Nothing here changes \
    the record; the kasl command line does that.",
    })
}

/// Whether a tool reaches past this machine to answer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Reads the local database only.
    Local,
    /// Asks a server too: the team's kasl-server, the company calendar.
    Network,
}

/// One entry of the tool list, with the shape of its arguments.
///
/// Every tool is read-only and says so: `readOnlyHint` is what lets a client
/// treat a call as safe to make without asking. `openWorldHint` is true only
/// for the tools that reach a server, so a client can tell a call that stays
/// on the machine from one that does not.
fn tool(name: &str, title: &str, description: &str, properties: Value, required: &[&str], reach: Reach) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
        "annotations": {
            "title": title,
            "readOnlyHint": true,
            "openWorldHint": reach == Reach::Network,
        },
    })
}

fn date_arg(description: &str) -> Value {
    json!({ "type": "string", "format": "date", "description": description })
}

fn flag_arg(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

fn names_arg(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": description })
}

/// The tools, in the order a session reaches for them: the day, then totals,
/// then the work in hand, then the connection.
fn tools() -> Vec<Value> {
    let sorts: Vec<String> = inbox::SortKey::value_variants()
        .iter()
        .filter_map(|key| key.to_possible_value().map(|value| value.get_name().to_string()))
        .collect();
    vec![
        tool(
            "day",
            "One day",
            "One day as `kasl report --json` prints it: when the workday began and ended (null for a day nothing was recorded on), the work intervals between pauses with short ones left out, their sum as work_seconds, productivity and whether it is below the configured minimum, the day's tasks, and the inbox counts.",
            json!({ "date": date_arg("The day, YYYY-MM-DD; today when omitted") }),
            &[],
            Reach::Local,
        ),
        tool(
            "pauses",
            "A day's pauses",
            "The pauses of one day as `kasl pauses list --json` prints them: each with its start, end, length and whether it was recorded by hand, and their total. Pauses shorter than min_minutes are left out unless kept by hand.",
            json!({
                "date": date_arg("The day, YYYY-MM-DD; today when omitted"),
                "min_minutes": { "type": "integer", "minimum": 0, "description": "The shortest pause to list, in minutes; the configured minimum when omitted" },
            }),
            &[],
            Reach::Local,
        ),
        tool(
            "week",
            "One week",
            "A week, Monday to Sunday: each recorded day's work time - the same work_seconds `day` gives it - and productivity, company rest days counted as full days, the total, the average per day and the average productivity. Asks the company calendar for rest days when the reporting API is configured.",
            json!({ "date": date_arg("Any day of the week, YYYY-MM-DD; this week when omitted") }),
            &[],
            Reach::Network,
        ),
        tool(
            "month",
            "One month",
            "A month as `kasl sum --json` prints it: each recorded day's work time and productivity, company rest days counted as full days, the total, the average per day and the average productivity. Asks the company calendar for rest days when the reporting API is configured.",
            json!({ "month": { "type": "string", "description": "The month, YYYY-MM; this month when omitted" } }),
            &[],
            Reach::Network,
        ),
        tool(
            "tasks",
            "Tasks",
            "Tasks as `kasl task list --json` prints them: id, the chain a task continues across days, when it was written, name, comment, completeness 0-100, tags, the Jira issue it came from. Give at most one of date, all, incomplete, tag and ids; today's tasks when none is given. Asking by ids for tasks that are not there is an error.",
            json!({
                "date": date_arg("Tasks written on this day, YYYY-MM-DD"),
                "all": flag_arg("Every task, from every day"),
                "incomplete": flag_arg("Tasks below 100% done, from every day"),
                "tag": { "type": "string", "description": "Tasks carrying this tag" },
                "ids": { "type": "array", "items": { "type": "integer" }, "description": "Tasks with these ids" },
            }),
            &[],
            Reach::Local,
        ),
        tool(
            "inbox",
            "The Jira inbox",
            "The Jira issues assigned to this person, as `kasl inbox list --json` prints them from the local table - never asking Jira, so the answer is as fresh as the watcher's last sync. Pinned issues lead, then by score and priority unless `sort` says otherwise; snoozed, dismissed and gone issues are left out unless asked for. `total` is the size of the inbox before the cuts.",
            json!({
                "limit": { "type": "integer", "minimum": 1, "description": "Only the first N issues after the cuts and the order" },
                "all": flag_arg("Include issues gone from Jira (closed or reassigned)"),
                "snoozed": flag_arg("Include issues still asleep"),
                "dismissed": flag_arg("Include dismissed issues"),
                "since": { "type": "string", "description": "Only issues first seen within a window: 12h, 1d, 7d, 2w" },
                "new": flag_arg("Only issues discovered in the last day"),
                "changed": { "type": "string", "description": "Only issues that visibly changed within a window: 12h, 1d, 7d, 2w" },
                "min_score": { "type": "number", "description": "Only issues whose ranking field is at least this" },
                "priority": { "type": "string", "description": "Only this priority, as Jira names it; a trailing + for it and above (High+)" },
                "status": names_arg("Only issues in any of these statuses, as Jira names them"),
                "type": names_arg("Only issues of any of these types: Bug, Story - as Jira names them"),
                "project": names_arg("Only issues of any of these projects: the prefix of the key"),
                "sort": { "type": "string", "enum": sorts, "description": "The order; pinned issues lead whatever it is" },
            }),
            &[],
            Reach::Local,
        ),
        tool(
            "issue",
            "One inbox issue",
            "One issue of the inbox in full, as `kasl inbox show --json` prints it: summary, status, priority, score, type, project, link, pin and dismissal with the rule behind them, when it was first and last seen, taken, snoozed until, gone and why. With `why`, where its place in the list comes from. An issue the inbox does not hold is an error.",
            json!({
                "key": { "type": "string", "description": "The issue key, as KA-12" },
                "why": flag_arg("Also say where the issue's place in the list comes from"),
            }),
            &["key"],
            Reach::Local,
        ),
        tool(
            "server_status",
            "Team server connection",
            "Whether this machine's connection to the team's kasl-server works, as `kasl server status --json` prints it: the address, whether the agent token is in the OS keyring, whether the server answers, its version, whose token it is, the first check that failed, and how the pulse is going. Asks the server; all null but `connected` when no server is connected.",
            json!({}),
            &[],
            Reach::Network,
        ),
        tool(
            "server_queue",
            "Days owed to the team server",
            "The days still waiting to reach the team's kasl-server, as `kasl server queue --json` prints them: each with when it was queued, how many times it was tried, and the last error. Read on this machine without asking the server; an empty list means nothing is owed.",
            json!({}),
            &[],
            Reach::Local,
        ),
    ]
}

/// Runs a call and wraps what it returns in a tool result.
async fn call_tool(params: &Value) -> Result<Value, Failure> {
    let name = params.get("name").and_then(Value::as_str).ok_or_else(|| Failure {
        code: INVALID_PARAMS,
        message: "a tool call needs a `name`".into(),
    })?;
    let args = match params.get("arguments") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(args)) => args.clone(),
        Some(_) => {
            return Err(Failure {
                code: INVALID_PARAMS,
                message: "a tool call's `arguments` is an object".into(),
            });
        }
    };

    // Anything the tool itself can go wrong with is a failed tool result
    // rather than a protocol error: the assistant is the one who can act on
    // "no issue KA-9", and a JSON-RPC error is reported to the client instead
    // of to the model.
    Ok(match isolated(run_tool(name.to_string(), args)).await {
        Ok(document) => json!({
            "content": [{ "type": "text", "text": document.to_string() }],
            "structuredContent": document,
        }),
        Err(err) => json!({
            "content": [{ "type": "text", "text": format!("{err:#}") }],
            "isError": true,
        }),
    })
}

/// Runs a read on its own task, so that a defect in it - a panic - fails that
/// one call instead of ending the session for every call after it.
async fn isolated<F>(read: F) -> Result<Value>
where
    F: Future<Output = Result<Value>> + Send + 'static,
{
    match tokio::spawn(read).await {
        Ok(outcome) => outcome,
        Err(join) if join.is_panic() => {
            let panic = join.into_panic();
            let reason = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("no reason given");
            bail!("kasl failed while reading this, which is a defect in kasl rather than in the request: {reason}")
        }
        Err(join) => bail!("the read was cancelled: {join}"),
    }
}

/// Answers one tool by name.
async fn run_tool(name: String, args: Map<String, Value>) -> Result<Value> {
    let args = Arguments { tool: &name, map: &args };
    let document = match name.as_str() {
        "day" => {
            args.only(&["date"])?;
            to_value(DayReport::read(args.date("date")?.unwrap_or_else(today))?.to_json())?
        }
        "pauses" => {
            args.only(&["date", "min_minutes"])?;
            let date = args.date("date")?.unwrap_or_else(today);
            to_value(DayPauses::read(date, args.whole("min_minutes")?)?.to_json())?
        }
        "week" => {
            args.only(&["date"])?;
            to_value(Summary::week(args.date("date")?.unwrap_or_else(today)).await?.to_week_json())?
        }
        "month" => {
            args.only(&["month"])?;
            to_value(Summary::month(args.month("month")?.unwrap_or_else(today)).await?.to_month_json())?
        }
        "tasks" => {
            args.only(&["date", "all", "incomplete", "tag", "ids"])?;
            to_value(tasks(&args)?)?
        }
        "inbox" => {
            args.only(&[
                "limit",
                "all",
                "snoozed",
                "dismissed",
                "since",
                "new",
                "changed",
                "min_score",
                "priority",
                "status",
                "type",
                "project",
                "sort",
            ])?;
            let include = Include {
                gone: args.flag("all")?,
                snoozed: args.flag("snoozed")?,
                dismissed: args.flag("dismissed")?,
            };
            let limit = args.whole("limit")?.map(|n| n as usize);
            to_value(inbox::list_document(limit, include, &inbox_filter(&args)?)?)?
        }
        "issue" => {
            args.only(&["key", "why"])?;
            let Some(key) = args.text("key")? else {
                bail!("`issue` needs a `key`, as KA-12; `inbox` lists them");
            };
            to_value(inbox::issue_document(&key, args.flag("why")?)?)?
        }
        "server_status" => {
            args.only(&[])?;
            to_value(server::status_document().await?)?
        }
        "server_queue" => {
            args.only(&[])?;
            to_value(server::queue_document()?)?
        }
        other => {
            let known: Vec<String> = tools().iter().filter_map(|t| t["name"].as_str().map(str::to_string)).collect();
            bail!("kasl serves no tool named `{other}`; it serves {}", known.join(", "))
        }
    };
    Ok(document)
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn to_value(document: impl serde::Serialize) -> Result<Value> {
    Ok(serde_json::to_value(document)?)
}

/// The tasks one call asks for: by ids, by tag, every incomplete one, all, or one day's.
fn tasks(args: &Arguments) -> Result<json::Tasks> {
    let date = args.date("date")?;
    let all = args.flag("all")?;
    let incomplete = args.flag("incomplete")?;
    let tag = args.text("tag")?;
    let ids = args.ids("ids")?;

    // Two selectors would have to be ranked, and whichever lost would be
    // ignored without a word: an answer to a question nobody asked.
    let given = [date.is_some(), all, incomplete, tag.is_some(), ids.is_some()]
        .iter()
        .filter(|given| **given)
        .count();
    if given > 1 {
        bail!("give one of `date`, `all`, `incomplete`, `tag` and `ids`: each chooses a different list");
    }

    let filter = match (ids, tag) {
        (Some(ids), _) => TaskFilter::ByIds(ids),
        (None, Some(tag)) => TaskFilter::ByTag(tag),
        (None, None) if all => TaskFilter::All,
        (None, None) if incomplete => TaskFilter::Incomplete,
        (None, None) => TaskFilter::Date(date.unwrap_or_else(today)),
    };
    let wanted = match &filter {
        TaskFilter::ByIds(ids) => Some(ids.clone()),
        _ => None,
    };
    let found = Tasks::new()?.fetch(filter)?;
    // Asked for by id, a task that is not there is a failure, as `task show` makes it.
    if let Some(ids) = wanted
        && found.is_empty()
    {
        return Err(msg_error_anyhow!(Message::TasksNotFoundForIds(ids)));
    }
    Ok(json::Tasks::of(&found))
}

/// The inbox cuts one call asks for, in the words `kasl inbox list` takes.
fn inbox_filter(args: &Arguments) -> Result<inbox::FilterArgs> {
    let since = args.text("since")?;
    let new = args.flag("new")?;
    if since.is_some() && new {
        bail!("`new` is the last day's `since`; give one of them");
    }
    let sort = match args.text("sort")? {
        Some(name) => Some(inbox::SortKey::from_str(&name, true).map_err(|_| {
            let known: Vec<String> = inbox::SortKey::value_variants()
                .iter()
                .filter_map(|key| key.to_possible_value().map(|value| value.get_name().to_string()))
                .collect();
            anyhow::anyhow!("`sort` is one of {}; got '{name}'", known.join(", "))
        })?),
        None => None,
    };
    Ok(inbox::FilterArgs {
        since,
        new,
        changed: args.text("changed")?,
        min_score: args.number("min_score")?,
        priority: args.text("priority")?,
        status: args.names("status")?,
        issue_type: args.names("type")?,
        project: args.names("project")?,
        sort,
    })
}

/// A call's arguments, read strictly.
///
/// A value of the wrong kind, or an argument the tool does not take, is
/// refused by name rather than ignored: a misspelt `dat` dropped on the floor
/// would answer about today while the model believes it asked about Monday.
struct Arguments<'a> {
    tool: &'a str,
    map: &'a Map<String, Value>,
}

impl Arguments<'_> {
    /// Refuses any argument but these.
    fn only(&self, known: &[&str]) -> Result<()> {
        let unknown: Vec<&str> = self.map.keys().map(String::as_str).filter(|key| !known.contains(key)).collect();
        if unknown.is_empty() {
            return Ok(());
        }
        let takes = match known {
            [] => "takes no arguments".to_string(),
            known => format!("takes {}", known.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")),
        };
        bail!(
            "`{}` {takes}; it does not know {}",
            self.tool,
            unknown.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ")
        )
    }

    /// The value, unless absent or null.
    fn get(&self, key: &str) -> Option<&Value> {
        self.map.get(key).filter(|value| !value.is_null())
    }

    fn text(&self, key: &str) -> Result<Option<String>> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
            Some(Value::String(text)) => Ok(Some(text.trim().to_string())),
            Some(other) => bail!("`{key}` is a string; got {other}"),
        }
    }

    fn flag(&self, key: &str) -> Result<bool> {
        match self.get(key) {
            None => Ok(false),
            Some(Value::Bool(flag)) => Ok(*flag),
            Some(other) => bail!("`{key}` is true or false; got {other}"),
        }
    }

    fn whole(&self, key: &str) -> Result<Option<u64>> {
        match self.get(key) {
            None => Ok(None),
            Some(value) => match value.as_u64() {
                Some(n) => Ok(Some(n)),
                None => bail!("`{key}` is a whole number, 0 or more; got {value}"),
            },
        }
    }

    fn number(&self, key: &str) -> Result<Option<f64>> {
        match self.get(key) {
            None => Ok(None),
            Some(value) => match value.as_f64() {
                Some(n) => Ok(Some(n)),
                None => bail!("`{key}` is a number; got {value}"),
            },
        }
    }

    /// A list of names; a single name is taken as a list of one.
    fn names(&self, key: &str) -> Result<Vec<String>> {
        match self.get(key) {
            None => Ok(Vec::new()),
            Some(Value::String(name)) => Ok(vec![name.trim().to_string()]),
            Some(Value::Array(names)) => names
                .iter()
                .map(|name| match name {
                    Value::String(name) => Ok(name.trim().to_string()),
                    other => bail!("`{key}` is a list of strings; it holds {other}"),
                })
                .collect(),
            Some(other) => bail!("`{key}` is a list of strings; got {other}"),
        }
    }

    fn ids(&self, key: &str) -> Result<Option<Vec<i32>>> {
        let Some(value) = self.get(key) else {
            return Ok(None);
        };
        let Value::Array(ids) = value else {
            bail!("`{key}` is a list of task ids; got {value}");
        };
        if ids.is_empty() {
            bail!("`{key}` is empty; give the ids to read, or leave it out for today's tasks");
        }
        ids.iter()
            .map(|id| {
                id.as_i64()
                    .and_then(|id| i32::try_from(id).ok())
                    .ok_or_else(|| anyhow::anyhow!("`{key}` holds {id}, which is not a task id"))
            })
            .collect::<Result<Vec<i32>>>()
            .map(Some)
    }

    fn date(&self, key: &str) -> Result<Option<NaiveDate>> {
        let Some(text) = self.text(key)? else {
            return Ok(None);
        };
        match NaiveDate::parse_from_str(&text, "%Y-%m-%d") {
            Ok(date) => Ok(Some(date)),
            Err(_) => bail!("`{key}` is a date, YYYY-MM-DD as 2026-10-21; got '{text}'"),
        }
    }

    /// A month, `YYYY-MM`, as its first day.
    fn month(&self, key: &str) -> Result<Option<NaiveDate>> {
        let Some(text) = self.text(key)? else {
            return Ok(None);
        };
        match NaiveDate::parse_from_str(&format!("{text}-01"), "%Y-%m-%d") {
            Ok(first) => Ok(Some(first)),
            Err(_) => bail!("`{key}` is a month, YYYY-MM as 2026-10; got '{text}'"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_named(name: &str) -> Value {
        tools()
            .into_iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("no tool named {name}"))
    }

    fn args(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn the_stage_serves_every_reading_the_plan_promised() {
        let names: Vec<String> = tools().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        for promised in ["day", "week", "month", "pauses", "tasks", "inbox", "issue", "server_status", "server_queue"] {
            assert!(names.contains(&promised.to_string()), "{promised} is missing from {names:?}");
        }
    }

    #[test]
    fn every_tool_declares_the_shape_of_its_arguments() {
        // A tool whose schema is wrong is called wrong, and the model has no
        // way to find out why: the schema is the only documentation it gets.
        for tool in tools() {
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object", "{}", tool["name"]);
            assert_eq!(schema["additionalProperties"], false, "{}", tool["name"]);
            for name in schema["required"].as_array().expect("required") {
                let name = name.as_str().unwrap();
                assert!(
                    schema["properties"].get(name).is_some(),
                    "{} requires `{name}` without describing it",
                    tool["name"]
                );
            }
            assert!(
                tool["description"].as_str().is_some_and(|d| d.len() > 60),
                "{} needs a description a model can act on",
                tool["name"]
            );
        }
    }

    #[test]
    fn every_tool_says_it_only_reads() {
        for tool in tools() {
            assert_eq!(tool["annotations"]["readOnlyHint"], true, "{}", tool["name"]);
        }
    }

    #[test]
    fn only_the_tools_that_ask_a_server_say_they_reach_past_the_machine() {
        for tool in tools() {
            let name = tool["name"].as_str().unwrap();
            let reaches = matches!(name, "week" | "month" | "server_status");
            assert_eq!(tool["annotations"]["openWorldHint"], reaches, "{name}");
        }
    }

    #[test]
    fn the_inbox_sorts_by_the_words_the_command_takes() {
        let sorts = &tool_named("inbox")["inputSchema"]["properties"]["sort"]["enum"];
        assert_eq!(sorts, &json!(["score", "priority", "new", "changed"]));
    }

    #[test]
    fn initialize_names_the_protocol_the_build_and_the_tools_capability() {
        let init = initialize();
        assert_eq!(init["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(init["serverInfo"]["name"], "kasl");
        assert_eq!(init["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
        // Without the tools capability a client never asks for the tools.
        assert!(init["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn a_notification_is_not_answered() {
        // The specification forbids a response to a message without an id;
        // answering one breaks clients that count messages.
        assert!(handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await.is_none());
    }

    #[tokio::test]
    async fn an_unknown_method_is_a_protocol_error_not_a_crash() {
        let response = handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "resources/list" })).await.unwrap();
        assert_eq!(response["error"]["code"], METHOD_NOT_FOUND);
        assert!(response["result"].is_null());
    }

    #[tokio::test]
    async fn a_call_without_a_name_is_a_protocol_error() {
        let response = handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {} }))
            .await
            .unwrap();
        assert_eq!(response["error"]["code"], INVALID_PARAMS);
    }

    #[tokio::test]
    async fn an_unknown_tool_answers_the_model_with_the_tools_there_are() {
        let call = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "add_task", "arguments": {} } });
        let response = handle(&call).await.unwrap();
        assert!(response["error"].is_null(), "{response}");
        assert_eq!(response["result"]["isError"], true);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("no tool named `add_task`") && text.contains("day"), "{text}");
    }

    #[tokio::test]
    async fn a_misspelt_argument_is_refused_rather_than_ignored() {
        // Ignored, `dat` would answer about today while the model believes it
        // asked about another day.
        let err = run_tool("day".into(), args(json!({ "dat": "2026-10-19" }))).await.unwrap_err().to_string();
        assert!(err.contains("`day` takes `date`") && err.contains("`dat`"), "{err}");
        let err = run_tool("server_queue".into(), args(json!({ "verbose": true }))).await.unwrap_err().to_string();
        assert!(err.contains("takes no arguments"), "{err}");
    }

    #[tokio::test]
    async fn a_value_of_the_wrong_kind_is_named() {
        let err = run_tool("day".into(), args(json!({ "date": "19.10.2026" }))).await.unwrap_err().to_string();
        assert!(err.contains("YYYY-MM-DD"), "{err}");
        let err = run_tool("month".into(), args(json!({ "month": "October" }))).await.unwrap_err().to_string();
        assert!(err.contains("YYYY-MM"), "{err}");
        let err = run_tool("inbox".into(), args(json!({ "all": "yes" }))).await.unwrap_err().to_string();
        assert!(err.contains("true or false"), "{err}");
        let err = run_tool("inbox".into(), args(json!({ "sort": "newest" }))).await.unwrap_err().to_string();
        assert!(err.contains("score, priority, new, changed"), "{err}");
        let err = run_tool("tasks".into(), args(json!({ "ids": [1, "two"] }))).await.unwrap_err().to_string();
        assert!(err.contains("not a task id"), "{err}");
    }

    #[tokio::test]
    async fn two_task_selectors_are_refused_rather_than_ranked() {
        let err = run_tool("tasks".into(), args(json!({ "all": true, "tag": "cli" })))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("give one of"), "{err}");
    }

    #[tokio::test]
    async fn a_panic_in_a_read_fails_that_call_and_not_the_session() {
        let err = isolated(async { panic!("a row nobody expected") }).await.unwrap_err().to_string();
        assert!(err.contains("defect in kasl") && err.contains("a row nobody expected"), "{err}");
        // The runtime is still there for the next call.
        assert_eq!(isolated(async { Ok(json!({ "ok": true })) }).await.unwrap(), json!({ "ok": true }));
    }

    #[test]
    fn a_month_is_read_as_its_first_day() {
        let map = args(json!({ "month": "2026-02" }));
        let month = Arguments { tool: "month", map: &map }.month("month").unwrap();
        assert_eq!(month, NaiveDate::from_ymd_opt(2026, 2, 1));
    }
}
