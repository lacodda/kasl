//! The config file one setting at a time: what `kasl config` reads and writes.
//!
//! A setting is named by its dotted key - `monitor.pause_threshold`,
//! `inbox.presets.urgent` - and the key is looked up in the schema
//! ([`super::schema`]), so the set of keys `kasl config` accepts is the set the
//! file can hold, and a value is checked against the very schema an editor
//! uses.
//!
//! **What is written is what was set.** An edit changes the file's JSON and
//! nothing else: the setting given, plus `$schema`. It does not round-trip
//! through [`Config`], which would write out every default of every block it
//! touched - and a default written into the file is no longer a default: it
//! reads as "set" and stops following kasl when the default changes.

use super::schema::{self, Findings, LEGACY_KEYS, Problem, SCHEMA_URL};
use super::{Config, InboxConfig, MonitorConfig, ProductivityConfig, ReportConfig, TaskDiscoveryConfig};
use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

/// Blocks a command writes, and the command that does.
///
/// `kasl config set` refuses them: the connection to kasl-server is checked
/// before it is kept, and only a plugin's manifest knows which of its
/// settings are secret - a token set here would sit in the file in plain text.
const OWNED: [(&str, &str); 2] = [
    (
        "kasl_server",
        "the connection is written by `kasl server connect`, and the pulse by `kasl server pulse enable` / `disable`",
    ),
    (
        "plugins",
        "a plugin's settings are written by `kasl plugin setup NAME` - only the plugin knows which of them are secret, and those go to the OS keyring",
    ),
];

/// Where a setting's value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Written in the file.
    Set,
    /// Not in the file; kasl uses its default.
    Default,
    /// Not in the file, and there is no default: the module is not set up.
    NotSet,
}

/// One setting as `kasl config show` lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Setting {
    pub key: String,
    /// What kasl uses; `None` when nothing is set and there is no default.
    pub value: Option<Value>,
    pub source: Source,
}

/// A dotted key, split into its segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key(Vec<String>);

impl Key {
    /// Reads `monitor.pause_threshold`; empty segments are refused.
    pub fn parse(text: &str) -> Result<Key> {
        let segments: Vec<String> = text.trim().split('.').map(str::to_string).collect();
        if segments.iter().any(|segment| segment.trim().is_empty()) {
            bail!("'{text}' is not a key; keys are dotted names, such as monitor.pause_threshold");
        }
        Ok(Key(segments))
    }

    pub fn segments(&self) -> &[String] {
        &self.0
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.join("."))
    }
}

/// The file as JSON; `None` when there is no file yet.
///
/// [`Config::read`] runs first, so the moves it makes on reading - GitLab's
/// old block to its plugin - have been made, and the file seen here is the one
/// any other command would see.
pub fn read_file(path: &Path) -> Result<Option<Value>> {
    Config::read().map_err(|error| anyhow::anyhow!("kasl cannot read {}: {error}", path.display()))?;
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    let doc: Value = serde_json::from_str(&text).map_err(|error| anyhow::anyhow!("{} is not valid JSON: {error}", path.display()))?;
    if !doc.is_object() {
        bail!("{} holds {}, not an object of settings", path.display(), kind_of(&doc));
    }
    Ok(Some(doc))
}

/// Writes the file: the settings as given, led by `$schema`.
pub fn write_file(path: &Path, doc: &Value) -> Result<()> {
    let mut doc = doc.clone();
    if let Value::Object(root) = &mut doc {
        root.insert("$schema".to_string(), Value::String(SCHEMA_URL.to_string()));
    }
    fs::write(path, serde_json::to_string_pretty(&doc)?)?;
    Ok(())
}

/// What kasl uses for each module it has defaults for, as JSON.
fn defaults() -> Value {
    let config = Config {
        monitor: Some(MonitorConfig::default()),
        productivity: Some(ProductivityConfig::default()),
        report: Some(ReportConfig::default()),
        task_discovery: Some(TaskDiscoveryConfig::default()),
        inbox: Some(InboxConfig::default()),
        ..Config::default()
    };
    serde_json::to_value(config).unwrap_or_else(|_| Value::Object(Map::new()))
}

/// Follows `$ref` to the definition it names.
fn resolve(node: &Value) -> &Value {
    let mut node = node;
    while let Some(target) = node.get("$ref").and_then(Value::as_str) {
        let Some(name) = target.strip_prefix("#/$defs/") else { break };
        match schema::document().get("$defs").and_then(|defs| defs.get(name)) {
            Some(definition) => node = definition,
            None => break,
        }
    }
    node
}

/// The schema of a named part of `node`: a property, a map entry, an item.
fn child<'a>(node: &'a Value, segment: &str) -> Option<&'a Value> {
    let node = resolve(node);
    if let Some(property) = node.get("properties").and_then(|properties| properties.get(segment)) {
        return Some(property);
    }
    match node.get("additionalProperties") {
        Some(Value::Bool(false)) | None => {}
        Some(entry) => return Some(entry),
    }
    if segment.parse::<usize>().is_ok() {
        return node.get("items");
    }
    None
}

/// The schema of a key, or why there is no such setting.
pub fn schema_of(key: &Key) -> Result<&'static Value> {
    let mut node = schema::document();
    for (depth, segment) in key.segments().iter().enumerate() {
        match child(node, segment) {
            Some(next) => node = next,
            None => {
                let typed = key.segments()[..=depth].join(".");
                bail!("`{typed}` is {}", unknown_key_message(&typed));
            }
        }
    }
    Ok(node)
}

/// Every key the schema names, down to the settings themselves.
///
/// Maps (`inbox.presets`, `plugins`) and lists end a key: their entries are the
/// user's own names.
pub fn known_keys() -> Vec<String> {
    fn walk(node: &Value, prefix: &str, out: &mut Vec<String>) {
        let node = resolve(node);
        let Some(Value::Object(properties)) = node.get("properties") else {
            return;
        };
        for (name, property) in properties {
            if name == "$schema" {
                continue;
            }
            let key = schema::join(prefix, name);
            out.push(key.clone());
            walk(property, &key, out);
        }
    }
    let mut keys = Vec::new();
    walk(schema::document(), "", &mut keys);
    keys
}

/// What to say about a key kasl does not know, with the one probably meant.
pub fn unknown_key_message(typed: &str) -> String {
    match suggestion(typed) {
        Some(meant) => format!("not a setting kasl knows - did you mean `{meant}`?"),
        None => "not a setting kasl knows - `kasl config show` lists them".to_string(),
    }
}

/// The known key closest to a mistyped one: a near spelling under the same
/// parent, or the same last name under another parent.
fn suggestion(typed: &str) -> Option<String> {
    let known = known_keys();
    let (parent, name) = typed.rsplit_once('.').unwrap_or(("", typed));
    let siblings = known.iter().filter(|key| key.rsplit_once('.').map(|(p, _)| p).unwrap_or("") == parent);
    let near = siblings
        .map(|key| (distance(key.rsplit('.').next().unwrap_or(key), name), key))
        .filter(|(d, _)| *d <= (name.chars().count() / 3).max(1))
        .min_by_key(|(d, _)| *d)
        .map(|(_, key)| key.clone());
    if near.is_some() {
        return near;
    }
    let mut elsewhere = known.iter().filter(|key| key.rsplit('.').next() == Some(name));
    match (elsewhere.next(), elsewhere.next()) {
        (Some(only), None) => Some(only.clone()),
        _ => None,
    }
}

/// Edit distance between two names.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = if ca == *cb { previous } else { 1 + previous.min(row[j]).min(row[j + 1]) };
            previous = current;
        }
    }
    row[b.len()]
}

/// The value at a key in a document.
fn at<'a>(doc: &'a Value, key: &Key) -> Option<&'a Value> {
    key.segments().iter().try_fold(doc, |node, segment| match node {
        Value::Object(map) => map.get(segment),
        Value::Array(items) => segment.parse::<usize>().ok().and_then(|index| items.get(index)),
        _ => None,
    })
}

/// What kasl uses for one setting, and where it comes from.
pub fn effective(file: Option<&Value>, key: &Key) -> Result<Setting> {
    let node = schema_of(key)?;
    let setting = |value: Option<Value>, source| Setting {
        key: key.to_string(),
        value,
        source,
    };
    let file = file.map(normalized);
    let defaults = defaults();
    if let Some(value) = file.as_ref().and_then(|doc| at(doc, key)) {
        return Ok(match at(&defaults, key) {
            Some(default) if value.is_object() && default.is_object() => setting(Some(merged(default, value)), Source::Set),
            _ => setting(Some(value.clone()), Source::Set),
        });
    }
    if let Some(value) = at(&defaults, key) {
        return Ok(setting(Some(value.clone()), Source::Default));
    }
    match resolve(node).get("default").or_else(|| node.get("default")) {
        Some(value) => Ok(setting(Some(value.clone()), Source::Default)),
        None => match resolve(node).get("type").and_then(Value::as_str) {
            Some("array") => Ok(setting(Some(Value::Array(Vec::new())), Source::Default)),
            Some("object") if resolve(node).get("properties").is_none() => Ok(setting(Some(Value::Object(Map::new())), Source::Default)),
            _ => Ok(setting(None, Source::NotSet)),
        },
    }
}

/// `over` laid over `under`, object by object.
fn merged(under: &Value, over: &Value) -> Value {
    match (under, over) {
        (Value::Object(under), Value::Object(over)) => {
            let mut out = under.clone();
            for (key, value) in over {
                let value = match out.get(key) {
                    Some(base) => merged(base, value),
                    None => value.clone(),
                };
                out.insert(key.clone(), value);
            }
            Value::Object(out)
        }
        (_, over) => over.clone(),
    }
}

/// Every setting, as `kasl config show` lists them: a module kasl has
/// defaults for is listed setting by setting; one it does not - a connection
/// - is one line until it is set up; the entries of a map are a line each.
pub fn settings(file: Option<&Value>) -> Vec<Setting> {
    let doc = file.map(normalized).unwrap_or_else(|| Value::Object(Map::new()));
    let defaults = defaults();
    let mut out = Vec::new();
    walk_settings(schema::document(), "", &doc, &defaults, &mut out);
    out
}

fn walk_settings(node: &Value, prefix: &str, doc: &Value, defaults: &Value, out: &mut Vec<Setting>) {
    let Some(Value::Object(properties)) = resolve(node).get("properties") else {
        return;
    };
    for (name, property) in properties {
        if name == "$schema" {
            continue;
        }
        let key = schema::join(prefix, name);
        let here = Key::parse(&key).expect("schema keys are dotted names");
        let in_file = at(doc, &here);
        let by_default = at(defaults, &here);
        let resolved = resolve(property);

        if resolved.get("properties").is_some() {
            if in_file.is_some() || by_default.is_some() {
                walk_settings(property, &key, doc, defaults, out);
            } else {
                out.push(Setting {
                    key,
                    value: None,
                    source: Source::NotSet,
                });
            }
            continue;
        }
        if resolved.get("type").and_then(Value::as_str) == Some("object")
            && let Some(Value::Object(entries)) = in_file
            && !entries.is_empty()
        {
            for (entry, value) in entries {
                out.push(Setting {
                    key: schema::join(&key, entry),
                    value: Some(value.clone()),
                    source: Source::Set,
                });
            }
            continue;
        }
        let (value, source) = match (in_file, by_default) {
            (Some(value), _) => (Some(value.clone()), Source::Set),
            (None, Some(value)) => (Some(value.clone()), Source::Default),
            (None, None) => match property.get("default").or_else(|| resolved.get("default")) {
                Some(value) => (Some(value.clone()), Source::Default),
                // A list or a map with nothing in it is empty by default, not
                // a module waiting to be set up.
                None => match resolved.get("type").and_then(Value::as_str) {
                    Some("array") => (Some(Value::Array(Vec::new())), Source::Default),
                    Some("object") => (Some(Value::Object(Map::new())), Source::Default),
                    _ => (None, Source::NotSet),
                },
            },
        };
        out.push(Setting { key, value, source });
    }
}

/// The file as kasl reads it: `jira_inbox` under its name since 1.19, and
/// retired keys left out.
pub fn normalized(doc: &Value) -> Value {
    let mut doc = doc.clone();
    normalize(&mut doc);
    doc
}

/// Brings a file written by an older kasl to the shape this one writes, and
/// says what changed.
fn normalize(doc: &mut Value) -> Vec<String> {
    let mut changed = Vec::new();
    let Value::Object(root) = doc else { return changed };
    if let Some(old) = root.remove("jira_inbox") {
        if root.contains_key("inbox") {
            changed.push("dropped `jira_inbox`: `inbox` is there, and it is the one kasl reads".to_string());
        } else {
            root.insert("inbox".to_string(), old);
            changed.push("moved `jira_inbox` to `inbox`, its name since 1.19".to_string());
        }
    }
    for (legacy, _) in LEGACY_KEYS {
        let key = Key::parse(legacy).expect("legacy keys are dotted names");
        if matches!(legacy, "jira_inbox" | "gitlab") {
            continue;
        }
        if remove(doc, &key) {
            changed.push(format!("dropped `{legacy}`, which nothing reads"));
        }
    }
    changed
}

/// Takes a key out of a document, and the objects it leaves empty.
fn remove(doc: &mut Value, key: &Key) -> bool {
    fn go(node: &mut Value, segments: &[String]) -> bool {
        let Value::Object(map) = node else { return false };
        match segments {
            [] => false,
            [last] => map.remove(last).is_some(),
            [first, rest @ ..] => {
                let Some(next) = map.get_mut(first) else { return false };
                let removed = go(next, rest);
                if removed && next.as_object().is_some_and(Map::is_empty) {
                    map.remove(first);
                }
                removed
            }
        }
    }
    go(doc, key.segments())
}

/// Puts a value at a key, making the objects on the way.
fn put(doc: &mut Value, key: &Key, value: Value) -> Result<()> {
    let mut node = doc;
    let (last, path) = key.segments().split_last().expect("a key has a segment");
    for segment in path {
        let Value::Object(map) = node else {
            bail!("`{key}` sits inside a value that is not an object");
        };
        node = map.entry(segment.clone()).or_insert_with(|| Value::Object(Map::new()));
    }
    match node {
        Value::Object(map) => {
            map.insert(last.clone(), value);
            Ok(())
        }
        _ => bail!("`{key}` sits inside a value that is not an object"),
    }
}

/// Refuses a key a command owns.
fn refuse_owned(key: &Key) -> Result<()> {
    if let Some((block, how)) = OWNED.iter().find(|(block, _)| key.segments()[0] == *block) {
        bail!("`{key}` is not set by hand: {how}. `kasl config get {block}` reads it");
    }
    Ok(())
}

/// Reads what was typed as the value a setting takes.
///
/// The text is tried as JSON first - `90`, `true`, `["a","b"]`,
/// `{"priority":"High+"}` - and then as a plain string, and the first reading
/// the setting's schema accepts is the one kept: `kasl config set
/// report.language en` needs no quotes, and `90` is a number where a number
/// is wanted.
pub fn value_for(key: &Key, text: &str) -> Result<Value> {
    let node = schema_of(key)?;
    let validator = schema::validator_at(node)?;
    let as_json = serde_json::from_str::<Value>(text).ok();
    let as_text = Value::String(text.to_string());
    let candidates: Vec<Value> = as_json.into_iter().chain(std::iter::once(as_text)).collect();
    if let Some(found) = candidates.iter().find(|candidate| validator.is_valid(candidate)) {
        return Ok(found.clone());
    }
    let wanted = describe(node);
    let reason = candidates
        .iter()
        .find(|candidate| !candidate.is_string() || wanted.contains("text"))
        .unwrap_or(&candidates[candidates.len() - 1]);
    let why = validator.iter_errors(reason).next().map(|error| error.to_string()).unwrap_or_default();
    bail!("`{key}` takes {wanted}; '{text}' is not one: {why}")
}

/// What kind of value a setting takes, in words.
fn describe(node: &Value) -> String {
    let resolved = resolve(node);
    if let Some(Value::Array(choices)) = resolved.get("enum").or_else(|| node.get("enum")) {
        let names: Vec<String> = choices
            .iter()
            .map(|c| c.as_str().map(str::to_string).unwrap_or_else(|| c.to_string()))
            .collect();
        return format!("one of {}", names.join(", "));
    }
    let range = |what: &str| {
        let low = resolved.get("minimum").or_else(|| node.get("minimum"));
        let high = resolved.get("maximum").or_else(|| node.get("maximum"));
        match (low, high) {
            (Some(low), Some(high)) => format!("{what} from {low} to {high}"),
            (Some(low), None) => format!("{what} of at least {low}"),
            (None, Some(high)) => format!("{what} of at most {high}"),
            (None, None) => what.to_string(),
        }
    };
    match resolved.get("type").and_then(Value::as_str) {
        Some("integer") => range("a whole number"),
        Some("number") => range("a number"),
        Some("boolean") => "true or false".to_string(),
        Some("string") => "text".to_string(),
        Some("array") => "a JSON list".to_string(),
        Some("object") => "a JSON object".to_string(),
        _ => "a value its schema accepts".to_string(),
    }
}

/// What an edit came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Edited {
    /// The setting before the edit, as kasl used it.
    pub before: Setting,
    /// The setting after it.
    pub after: Setting,
    /// What the edit did to keys an older kasl wrote.
    pub tidied: Vec<String>,
    /// Problems the file already had, elsewhere; the edit neither made nor fixed them.
    pub elsewhere: Vec<Problem>,
}

/// Sets a key in the file at `path`, or refuses with why.
///
/// Refused when the key is not a setting, when a command owns it, when the
/// value is not one the setting takes, and when the edit would leave the
/// file with a problem it did not have before. Problems it already had,
/// elsewhere, do not stop the edit; they come back in [`Edited::elsewhere`].
pub fn set(path: &Path, key: &Key, text: &str) -> Result<Edited> {
    refuse_owned(key)?;
    let value = value_for(key, text)?;
    edit(path, key, |doc| put(doc, key, value))
}

/// Takes a key out of the file at `path`, so kasl goes back to its default.
pub fn unset(path: &Path, key: &Key) -> Result<Edited> {
    refuse_owned(key)?;
    schema_of(key)?;
    edit(path, key, |doc| {
        remove(doc, key);
        Ok(())
    })
}

fn edit(path: &Path, key: &Key, change: impl FnOnce(&mut Value) -> Result<()>) -> Result<Edited> {
    let mut doc = read_file(path)?.unwrap_or_else(|| Value::Object(Map::new()));
    let before = effective(Some(&doc), key)?;
    let found_before = schema::check(&normalized(&doc));

    let tidied = normalize(&mut doc);
    change(&mut doc)?;
    let found_after = schema::check(&doc);

    // Only what this edit would bring in stops it: a file that already had a
    // typo elsewhere still takes a new setting.
    let new: Vec<&Problem> = found_after.problems.iter().filter(|p| !found_before.problems.contains(p)).collect();
    if !new.is_empty() {
        let lines: Vec<String> = new.iter().map(|p| format!("  {p}")).collect();
        bail!("`{key}` was not changed - the file would not read as written:\n{}", lines.join("\n"));
    }

    write_file(path, &doc)?;
    let after = effective(Some(&doc), key)?;
    let Findings { problems, .. } = found_after;
    Ok(Edited {
        before,
        after,
        tidied,
        elsewhere: problems,
    })
}

/// "a number", "an object", for a message about a value.
fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "true or false",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn key(text: &str) -> Key {
        Key::parse(text).unwrap()
    }

    #[test]
    fn a_typed_value_becomes_what_the_setting_takes() {
        assert_eq!(value_for(&key("monitor.pause_threshold"), "90").unwrap(), json!(90));
        assert_eq!(value_for(&key("report.language"), "ru").unwrap(), json!("ru"));
        assert_eq!(value_for(&key("inbox.notify_gone"), "true").unwrap(), json!(true));
        assert_eq!(
            value_for(&key("inbox.presets.urgent"), r#"{"priority":"High+","since":"7d"}"#).unwrap(),
            json!({ "priority": "High+", "since": "7d" })
        );
        assert_eq!(value_for(&key("report.output_dir"), "123").unwrap(), json!("123"), "text where text is wanted");
    }

    #[test]
    fn a_value_the_setting_does_not_take_is_refused_with_what_it_takes() {
        let refused = value_for(&key("monitor.pause_threshold"), "soon").unwrap_err().to_string();
        assert!(refused.contains("a whole number of at least 1"), "{refused}");
        let refused = value_for(&key("inbox.poll_interval_secs"), "5").unwrap_err().to_string();
        assert!(refused.contains("at least 30"), "{refused}");
        let refused = value_for(&key("report.language"), "de").unwrap_err().to_string();
        assert!(refused.contains("one of en, ru"), "{refused}");
    }

    #[test]
    fn an_unknown_key_is_refused_with_the_one_meant() {
        let refused = schema_of(&key("monitor.pause_treshold")).unwrap_err().to_string();
        assert!(refused.contains("did you mean `monitor.pause_threshold`"), "{refused}");
        let refused = schema_of(&key("pause_threshold")).unwrap_err().to_string();
        assert!(
            refused.contains("did you mean `monitor.pause_threshold`"),
            "a bare name finds its module: {refused}"
        );
    }

    #[test]
    fn a_default_is_said_to_be_one_until_it_is_set() {
        let file = json!({ "monitor": { "pause_threshold": 90 } });
        let set = effective(Some(&file), &key("monitor.pause_threshold")).unwrap();
        assert_eq!((set.value, set.source), (Some(json!(90)), Source::Set));
        let default = effective(Some(&file), &key("monitor.min_pause_duration")).unwrap();
        assert_eq!((default.value, default.source), (Some(json!(20)), Source::Default));
        let schema_default = effective(None, &key("report.language")).unwrap();
        assert_eq!((schema_default.value, schema_default.source), (Some(json!("en")), Source::Default));
        let absent = effective(None, &key("jira.api_url")).unwrap();
        assert_eq!(absent.source, Source::NotSet);
    }

    #[test]
    fn a_module_without_defaults_is_one_line_until_it_is_set_up() {
        let listed = settings(None);
        let jira: Vec<&Setting> = listed.iter().filter(|s| s.key.starts_with("jira")).collect();
        assert_eq!(jira.len(), 1, "{jira:?}");
        assert_eq!(jira[0].source, Source::NotSet);
        assert!(listed.iter().any(|s| s.key == "monitor.pause_threshold" && s.source == Source::Default));
    }

    #[test]
    fn removing_the_last_key_of_a_block_removes_the_block() {
        let mut doc = json!({ "monitor": { "pause_threshold": 90 }, "report": { "language": "ru" } });
        assert!(remove(&mut doc, &key("monitor.pause_threshold")));
        assert_eq!(doc, json!({ "report": { "language": "ru" } }));
        assert!(!remove(&mut doc, &key("monitor.pause_threshold")), "nothing left to take");
    }

    #[test]
    fn an_older_files_keys_take_their_new_shape() {
        let mut doc = json!({ "jira_inbox": { "notify": false }, "server": { "api_url": "x", "auth_token": "t" } });
        let changed = normalize(&mut doc);
        assert_eq!(doc, json!({ "inbox": { "notify": false } }));
        assert_eq!(changed.len(), 2, "{changed:?}");
    }

    #[test]
    fn blocks_a_command_owns_are_not_set_by_hand() {
        assert!(
            refuse_owned(&key("kasl_server.pulse"))
                .unwrap_err()
                .to_string()
                .contains("kasl server pulse enable")
        );
        assert!(
            refuse_owned(&key("plugins.gitlab.settings.token"))
                .unwrap_err()
                .to_string()
                .contains("kasl plugin setup")
        );
        assert!(refuse_owned(&key("monitor.pause_threshold")).is_ok());
    }
}
