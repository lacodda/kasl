//! The config file's JSON Schema, and what is wrong with a file by it.
//!
//! The schema is generated from the config types themselves, so the schema an
//! editor checks the file against, the one `kasl config` validates with and
//! the struct kasl reads cannot say three different things. It is published
//! at [`SCHEMA_URL`], and every file kasl writes names it in `$schema`.
//!
//! Two passes find what is wrong with a file. The schema finds every key that
//! is not a setting, every value of the wrong type or out of range, every
//! required field missing. Then the parts the schema cannot express - an inbox
//! rule that does nothing, a window that is not `7d` - are checked by the same
//! functions that read them at run time, on the parts the schema passed.

use super::Config;
use crate::libs::inbox_filter::{InboxPreset, parse_window};
use crate::libs::inbox_rules::InboxRule;
use schemars::generate::SchemaSettings;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::sync::OnceLock;

/// Where the schema is published: the docs site serves the copy kept in
/// `docs/public/config.schema.json`, which a test holds equal to [`document`].
pub const SCHEMA_URL: &str = "https://kasl.lacodda.com/config.schema.json";

/// The config file's JSON Schema, generated once per run.
pub fn document() -> &'static Value {
    static DOCUMENT: OnceLock<Value> = OnceLock::new();
    DOCUMENT.get_or_init(build)
}

fn build() -> Value {
    let schema = SchemaSettings::draft2020_12().into_generator().into_root_schema_for::<Config>();
    let mut value = schema.to_value();
    tidy(&mut value);
    if let Value::Object(root) = &mut value {
        root.insert("$id".to_string(), json!(SCHEMA_URL));
        if let Some(Value::Object(properties)) = root.get_mut("properties") {
            properties.insert(
                "$schema".to_string(),
                json!({ "type": "string", "description": "The schema this file follows. kasl writes it, so an editor checks every key." }),
            );
        }
    }
    value
}

/// Shapes the generated schema into the one the file is held to.
///
/// - **No null.** An `Option` is written by leaving the key out, never as
///   `null`; a null branch would also turn every mistake inside a block into
///   "matches none of the schemas in anyOf" instead of naming the field.
/// - **No unknown keys.** Every object with named properties refuses others:
///   at run time an unknown key is ignored, which is exactly why a typo needs
///   saying. One rule here rather than an attribute on each struct, so a new
///   struct cannot forget it.
/// - **Choices as `enum`.** A unit enum with documented variants comes out
///   as a `oneOf` of constants; as an `enum` an error lists the choices, and
///   the descriptions stay for editors that show them.
fn tidy(node: &mut Value) {
    let Value::Object(map) = node else { return };

    if let Some(Value::Array(types)) = map.get_mut("type") {
        types.retain(|t| t != "null");
        if types.len() == 1 {
            let only = types.remove(0);
            map.insert("type".to_string(), only);
        }
    }
    if let Some(Value::Array(values)) = map.get_mut("enum") {
        values.retain(|v| !v.is_null());
    }
    if map.get("default").is_some_and(Value::is_null) {
        map.remove("default");
    }
    for keyword in ["anyOf", "oneOf"] {
        let Some(Value::Array(branches)) = map.get(keyword) else { continue };
        let kept: Vec<Value> = branches.iter().filter(|branch| **branch != json!({ "type": "null" })).cloned().collect();
        if kept.len() == branches.len() {
            continue;
        }
        map.remove(keyword);
        match <[Value; 1]>::try_from(kept) {
            Ok([Value::Object(only)]) => {
                for (key, value) in only {
                    map.entry(key).or_insert(value);
                }
            }
            Ok([other]) => {
                map.insert(keyword.to_string(), Value::Array(vec![other]));
            }
            Err(kept) => {
                map.insert(keyword.to_string(), Value::Array(kept));
            }
        }
    }
    if let Some(Value::Array(branches)) = map.get("oneOf")
        && !branches.is_empty()
        && branches.iter().all(|branch| branch.get("const").is_some())
    {
        let values: Vec<Value> = branches.iter().filter_map(|branch| branch.get("const").cloned()).collect();
        let descriptions: Vec<Value> = branches
            .iter()
            .map(|branch| branch.get("description").cloned().unwrap_or_else(|| json!("")))
            .collect();
        let kind = branches.first().and_then(|branch| branch.get("type")).cloned();
        map.remove("oneOf");
        map.insert("enum".to_string(), Value::Array(values));
        map.insert("enumDescriptions".to_string(), Value::Array(descriptions));
        if let Some(kind) = kind {
            map.entry("type".to_string()).or_insert(kind);
        }
    }
    if map.contains_key("properties") && !map.contains_key("additionalProperties") {
        map.insert("additionalProperties".to_string(), Value::Bool(false));
    }

    // Into the subschemas only: a `properties` map is keyed by field names,
    // and a field may well be called `type`.
    for keyword in ["properties", "$defs"] {
        if let Some(Value::Object(children)) = map.get_mut(keyword) {
            children.values_mut().for_each(tidy);
        }
    }
    for keyword in ["additionalProperties", "items"] {
        if let Some(child) = map.get_mut(keyword) {
            tidy(child);
        }
    }
    for keyword in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        if let Some(Value::Array(children)) = map.get_mut(keyword) {
            children.iter_mut().for_each(tidy);
        }
    }
}

/// One thing a file says that kasl does not read as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// The setting, as `kasl config get` takes it: `monitor.pause_threshold`,
    /// `inbox.rules.2`; empty for the file as a whole.
    pub key: String,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.key.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.key, self.message)
        }
    }
}

/// What a file comes to: what is wrong with it, and what is only old.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Findings {
    /// Settings that do not do what they say: a key kasl does not know, a
    /// value it cannot use, a rule that is skipped.
    pub problems: Vec<Problem>,
    /// Keys an older kasl wrote: read under their new name, or ignored, and
    /// gone from the file at its next save. Nothing to fix.
    pub notices: Vec<Problem>,
}

/// Keys an older kasl wrote, and what has become of them.
///
/// Said as notices rather than problems: none of them changes what kasl does
/// now, and each leaves the file the next time kasl saves it.
pub const LEGACY_KEYS: [(&str, &str); 4] = [
    (
        "server",
        "retired in 1.21: nothing read it - `report --send` goes through the `si` block. The next save drops it",
    ),
    (
        "jira.completed_statuses",
        "retired in 1.21: issue discovery stopped reading it. The next save drops it",
    ),
    (
        "jira_inbox",
        "the name of `inbox` before 1.19: still read as `inbox`, and the next save writes it under that name",
    ),
    (
        "gitlab",
        "GitLab's own block before 1.20: it moves to the GitLab plugin once the OS keyring takes its token",
    ),
];

/// What is wrong with a config file, read as JSON.
pub fn check(doc: &Value) -> Findings {
    let mut findings = Findings::default();
    let validator = validator();

    for error in validator.iter_errors(doc) {
        let key = pointer_to_key(&error.instance_path().to_string());
        match error.kind() {
            jsonschema::error::ValidationErrorKind::AdditionalProperties { unexpected } => {
                for name in unexpected {
                    let full = join(&key, name);
                    match LEGACY_KEYS.iter().find(|(legacy, _)| *legacy == full) {
                        Some((_, note)) => findings.notices.push(Problem {
                            key: full,
                            message: (*note).to_string(),
                        }),
                        None => findings.problems.push(Problem {
                            message: super::keys::unknown_key_message(&full),
                            key: full,
                        }),
                    }
                }
            }
            jsonschema::error::ValidationErrorKind::AnyOf { .. } => findings.problems.push(Problem {
                message: format!("{} is not one of the forms this setting takes", error.instance()),
                key,
            }),
            _ => findings.problems.push(Problem {
                message: error.to_string(),
                key,
            }),
        }
    }

    check_semantics(doc, &mut findings.problems);

    // The schema passing is not the whole promise: the file is read by serde,
    // and a value the schema allows (a whole number written as 30.0) can
    // still fail there. Said only when nothing above explains it.
    if findings.problems.is_empty()
        && let Err(error) = serde_json::from_value::<Config>(without_legacy(doc))
    {
        findings.problems.push(Problem {
            key: String::new(),
            message: format!("kasl cannot read this file: {error}"),
        });
    }
    findings
}

/// What the schema cannot say: rules and presets that read but would act
/// other than as written, and the windows the inbox counts in.
///
/// Only parts the schema passed are looked at, so one mistake is said once.
fn check_semantics(doc: &Value, problems: &mut Vec<Problem>) {
    let Some(inbox) = doc.get("inbox").or_else(|| doc.get("jira_inbox")) else {
        return;
    };
    let under = |key: &str, problems: &[Problem]| problems.iter().any(|p| p.key == key || p.key.starts_with(&format!("{key}.")));

    if let Some(window) = inbox.get("toast_snooze_for").and_then(Value::as_str)
        && !under("inbox.toast_snooze_for", problems)
        && let Err(error) = parse_window(window)
    {
        problems.push(Problem {
            key: "inbox.toast_snooze_for".to_string(),
            message: error.to_string(),
        });
    }
    if let Some(Value::Array(rules)) = inbox.get("rules") {
        for (index, rule) in rules.iter().enumerate() {
            let key = format!("inbox.rules.{index}");
            if under(&key, problems) {
                continue;
            }
            if let Err(reason) = InboxRule::parse(rule) {
                problems.push(Problem {
                    key,
                    message: format!("this rule is skipped: {reason}"),
                });
            }
        }
    }
    if let Some(Value::Object(presets)) = inbox.get("presets") {
        for (name, preset) in presets {
            let key = format!("inbox.presets.{name}");
            if under(&key, problems) {
                continue;
            }
            if let Err(reason) = InboxPreset::parse(preset) {
                problems.push(Problem {
                    key,
                    message: format!("`--preset {name}` would fail: {reason}"),
                });
            }
        }
    }
}

/// The document with the keys an older kasl wrote taken out, the way
/// [`Config`] reads past them.
fn without_legacy(doc: &Value) -> Value {
    let mut doc = doc.clone();
    if let Value::Object(root) = &mut doc {
        root.remove("$schema");
        root.remove("server");
        root.remove("gitlab");
        if let Some(Value::Object(jira)) = root.get_mut("jira") {
            jira.remove("completed_statuses");
        }
    }
    doc
}

/// The validator for the whole file.
pub(crate) fn validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| jsonschema::validator_for(document()).expect("the generated config schema is a valid schema"))
}

/// A validator for one part of the schema: `node` with the definitions its
/// references point into.
pub(crate) fn validator_at(node: &Value) -> anyhow::Result<jsonschema::Validator> {
    let mut schema = match node {
        Value::Object(map) => map.clone(),
        Value::Bool(true) => Map::new(),
        other => anyhow::bail!("not a schema: {other}"),
    };
    if let Some(definitions) = document().get("$defs") {
        schema.insert("$defs".to_string(), definitions.clone());
    }
    jsonschema::validator_for(&Value::Object(schema)).map_err(|error| anyhow::anyhow!("{error}"))
}

/// `/inbox/rules/2` as the dotted key `inbox.rules.2`.
pub(crate) fn pointer_to_key(pointer: &str) -> String {
    pointer
        .split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>()
        .join(".")
}

/// A key under a parent key, which may be the root.
pub(crate) fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() { name.to_string() } else { format!("{parent}.{name}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems(doc: Value) -> Vec<String> {
        check(&doc).problems.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_partial_block_is_a_whole_config() {
        assert!(problems(json!({ "monitor": { "pause_threshold": 90 } })).is_empty());
        let config: Config = serde_json::from_value(json!({ "monitor": { "pause_threshold": 90 } })).unwrap();
        assert_eq!(config.monitor.unwrap().min_pause_duration, 20, "the rest comes from the defaults");
    }

    #[test]
    fn a_misspelt_key_is_named_with_the_one_meant() {
        let found = problems(json!({ "monitor": { "pause_treshold": 90 } }));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("monitor.pause_treshold: "), "{found:?}");
        assert!(found[0].contains("monitor.pause_threshold"), "the suggestion names the real key: {found:?}");
    }

    #[test]
    fn a_value_of_the_wrong_kind_or_out_of_range_is_named() {
        let found = problems(json!({ "monitor": { "pause_threshold": "soon" }, "inbox": { "poll_interval_secs": 5 } }));
        assert!(found.iter().any(|p| p.starts_with("monitor.pause_threshold: ")), "{found:?}");
        assert!(found.iter().any(|p| p.starts_with("inbox.poll_interval_secs: ")), "{found:?}");
    }

    #[test]
    fn a_rule_that_does_nothing_is_named_by_its_place() {
        let found = problems(json!({ "inbox": { "rules": [
            { "when": { "priority": "High+" }, "then": { "pin": true } },
            { "when": { "priority": "High+" }, "then": {} }
        ] } }));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("inbox.rules.1: this rule is skipped"), "{found:?}");
    }

    #[test]
    fn a_misspelt_rule_key_is_said_once() {
        let found = problems(json!({ "inbox": { "rules": [ { "when": { "prority": "High+" }, "then": { "pin": true } } ] } }));
        assert_eq!(found.len(), 1, "one mistake, one line: {found:?}");
        assert!(found[0].starts_with("inbox.rules.0.when.prority: "), "{found:?}");
    }

    #[test]
    fn a_preset_with_a_bad_window_is_named() {
        let found = problems(json!({ "inbox": { "presets": { "urgent": { "since": "a week" } } } }));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("inbox.presets.urgent: `--preset urgent` would fail"), "{found:?}");
    }

    #[test]
    fn keys_an_older_kasl_wrote_are_notices_not_problems() {
        let findings = check(&json!({
            "server": { "api_url": "https://reports.example.com", "auth_token": "t" },
            "jira": { "login": "me", "api_url": "https://jira.example.com", "completed_statuses": [] },
            "jira_inbox": { "notify": false }
        }));
        assert!(findings.problems.is_empty(), "{:?}", findings.problems);
        let mut keys: Vec<&str> = findings.notices.iter().map(|n| n.key.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["jira.completed_statuses", "jira_inbox", "server"]);
    }

    #[test]
    fn the_file_kasl_writes_passes_its_own_schema() {
        let config = Config {
            monitor: Some(Default::default()),
            productivity: Some(Default::default()),
            inbox: Some(Default::default()),
            task_discovery: Some(Default::default()),
            ..Config::default()
        };
        let mut doc = serde_json::to_value(&config).unwrap();
        doc.as_object_mut().unwrap().insert("$schema".to_string(), json!(SCHEMA_URL));
        assert_eq!(check(&doc), Findings::default());
    }

    #[test]
    fn null_is_not_a_way_to_unset() {
        let found = problems(json!({ "monitor": null }));
        assert_eq!(found.len(), 1, "{found:?}");
    }
}
