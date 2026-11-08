//! `kasl config`: the settings file, one setting at a time.
//!
//! `kasl setup` walks whole modules; this changes one threshold without
//! opening `config.json`, and checks the value against the file's JSON
//! Schema before writing it. The same schema is what an editor checks the
//! file against, and what `kasl config check` reports by.
//!
//! ```bash
//! kasl config                                   # every setting, and where it comes from
//! kasl config get monitor.pause_threshold
//! kasl config set monitor.pause_threshold 90
//! kasl config set inbox.presets.urgent '{"priority":"High+","since":"7d"}'
//! kasl config unset monitor.pause_threshold     # back to the default
//! kasl config check                             # what the file says that kasl does not read
//! ```

use crate::libs::config::Config;
use crate::libs::config::keys::{self, Edited, Key, Setting, Source};
use crate::libs::config::schema;
use crate::libs::daemon;
use crate::libs::json;
use crate::libs::messages::Message;
use crate::libs::output::{self, Format};
use crate::libs::view::View;
use crate::{msg_info, msg_print, msg_success, msg_warning};
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use serde_json::Value;

/// Command-line arguments for `kasl config`.
#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    command: Option<ConfigCommand>,

    #[command(flatten)]
    format: Format,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Every setting, with the value kasl uses and whether it is set or a default
    #[command(about = "List every setting, set or default")]
    Show {
        #[command(flatten)]
        format: Format,
    },

    /// One setting's value: what kasl uses, set or default
    #[command(about = "Print one setting")]
    Get {
        /// The setting, as a dotted key: monitor.pause_threshold
        #[arg(value_name = "KEY")]
        key: String,

        #[command(flatten)]
        format: Format,
    },

    /// Change one setting, checked against the schema before it is written
    #[command(about = "Change one setting")]
    Set {
        /// The setting, as a dotted key: monitor.pause_threshold
        #[arg(value_name = "KEY")]
        key: String,

        /// The value: 90, true, ru - or JSON for a list or an object
        #[arg(value_name = "VALUE", allow_hyphen_values = true)]
        value: String,
    },

    /// Take a setting out of the file, so kasl goes back to its default
    #[command(about = "Take a setting back to its default")]
    Unset {
        /// The setting, as a dotted key: monitor.pause_threshold
        #[arg(value_name = "KEY")]
        key: String,
    },

    /// Name everything in the file that kasl does not read as written
    #[command(about = "Check the file: unknown keys, wrong values, skipped rules")]
    Check {
        #[command(flatten)]
        format: Format,
    },

    /// Print where the config file lives
    #[command(about = "Print the config file's path")]
    Path,

    /// Print the JSON Schema the file follows
    #[command(about = "Print the config file's JSON Schema")]
    Schema,
}

/// Entry point for `kasl config`.
pub fn cmd(args: ConfigArgs) -> Result<()> {
    if args.format.json && args.command.is_some() {
        bail!("--json before a subcommand applies to the list; give it to the subcommand: `kasl config get KEY --json`");
    }
    match args.command {
        None => show(args.format.json),
        Some(ConfigCommand::Show { format }) => show(format.json),
        Some(ConfigCommand::Get { key, format }) => get(&key, format.json),
        Some(ConfigCommand::Set { key, value }) => set(&key, &value),
        Some(ConfigCommand::Unset { key }) => unset(&key),
        Some(ConfigCommand::Check { format }) => check(format.json),
        Some(ConfigCommand::Path) => {
            println!("{}", Config::path()?.display());
            Ok(())
        }
        Some(ConfigCommand::Schema) => output::print_json(schema::document()),
    }
}

/// Lists every setting: what kasl uses, and whether it is set or a default.
fn show(json: bool) -> Result<()> {
    let path = Config::path()?;
    let file = keys::read_file(&path)?;
    let settings = keys::settings(file.as_ref());
    let findings = file.as_ref().map(schema::check).unwrap_or_default();

    if json {
        return output::print_json(&json::ConfigShow {
            file: path.display().to_string(),
            exists: file.is_some(),
            settings: settings.iter().map(json::ConfigSetting::from).collect(),
            problems: findings.problems.iter().map(json::ConfigProblem::from).collect(),
            notices: findings.notices.iter().map(json::ConfigProblem::from).collect(),
        });
    }

    match file {
        Some(_) => msg_print!(Message::ConfigFileAt(path.display().to_string()), true),
        None => msg_print!(Message::ConfigFileNone(path.display().to_string()), true),
    }
    View::config_settings(&settings)?;
    if !findings.problems.is_empty() {
        msg_warning!(Message::ConfigHasProblems(findings.problems.len()));
    }
    Ok(())
}

/// Prints one setting's value; a setting that is not set fails, so a script can tell.
fn get(key: &str, json: bool) -> Result<()> {
    let key = Key::parse(key)?;
    let file = keys::read_file(&Config::path()?)?;
    let setting = keys::effective(file.as_ref(), &key)?;
    if json {
        return output::print_json(&json::ConfigSetting::from(&setting));
    }
    match &setting.value {
        Some(Value::String(text)) => println!("{text}"),
        Some(value @ (Value::Object(_) | Value::Array(_))) => println!("{}", serde_json::to_string_pretty(value)?),
        Some(value) => println!("{value}"),
        None => bail!("{}", Message::ConfigNotSet(key.to_string())),
    }
    Ok(())
}

/// Changes one setting, or says why not and leaves the file as it was.
fn set(key: &str, value: &str) -> Result<()> {
    let key = Key::parse(key)?;
    let path = Config::path()?;
    let had_inbox = keys::read_file(&path)?.is_some_and(|doc| doc.get("inbox").or_else(|| doc.get("jira_inbox")).is_some());
    let edited = keys::set(&path, &key, value)?;

    if edited.before.source == Source::Set && edited.before.value == edited.after.value {
        msg_info!(Message::ConfigUnchanged {
            key: key.to_string(),
            value: shown(edited.after.value.as_ref()),
        });
    } else {
        msg_success!(Message::ConfigChanged {
            key: key.to_string(),
            value: shown(edited.after.value.as_ref()),
            was: was(&edited.before),
        });
    }
    // The inbox block being there is what turns Jira's poll on, so a first
    // inbox setting - a preset, say - starts it; that must not go unsaid.
    if !had_inbox && key.segments()[0] == "inbox" && Config::read()?.jira.is_some() {
        msg_info!(Message::ConfigInboxStartsJiraPoll);
    }
    after_edit(&key, &edited)
}

/// Takes one setting out of the file.
fn unset(key: &str) -> Result<()> {
    let key = Key::parse(key)?;
    let edited = keys::unset(&Config::path()?, &key)?;
    if edited.before.source != Source::Set {
        msg_info!(Message::ConfigWasNotSet(key.to_string()));
        return Ok(());
    }
    match (&edited.after.source, &edited.after.value) {
        (Source::Default, Some(value)) => msg_success!(Message::ConfigBackToDefault {
            key: key.to_string(),
            value: shown(Some(value)),
        }),
        _ => msg_success!(Message::ConfigRemoved(key.to_string())),
    }
    after_edit(&key, &edited)
}

/// What every edit says after its own line: what it tidied, what it left,
/// and the watcher brought round to a setting it reads only when it starts.
fn after_edit(key: &Key, edited: &Edited) -> Result<()> {
    for line in &edited.tidied {
        msg_info!(Message::ConfigTidied(line.clone()));
    }
    if !edited.elsewhere.is_empty() {
        msg_warning!(Message::ConfigOtherProblems(edited.elsewhere.len()));
    }
    // The monitor's thresholds are read once, when the watcher starts; every
    // other block is read again each time it is used.
    if key.segments()[0] == "monitor" && daemon::is_running() {
        msg_info!(Message::WatcherStoppingForConfig);
        daemon::stop()?;
        msg_info!(Message::WatcherRestartingAfterConfig);
        match daemon::spawn() {
            Ok(()) => msg_success!(Message::WatcherRestarted),
            Err(e) => msg_warning!(Message::WatcherRestartFailed { error: e.to_string() }),
        }
    }
    Ok(())
}

/// Names everything in the file that kasl does not read as written, and
/// fails when there is any of it.
fn check(json: bool) -> Result<()> {
    let path = Config::path()?;
    let findings = match keys::read_file(&path)? {
        Some(doc) => schema::check(&doc),
        None => Default::default(),
    };
    if json {
        output::print_json(&json::ConfigCheck {
            file: path.display().to_string(),
            problems: findings.problems.iter().map(json::ConfigProblem::from).collect(),
            notices: findings.notices.iter().map(json::ConfigProblem::from).collect(),
        })?;
    } else {
        for notice in &findings.notices {
            msg_info!(Message::ConfigNotice(notice.to_string()));
        }
        for problem in &findings.problems {
            msg_warning!(Message::ConfigProblem(problem.to_string()));
        }
        if findings.problems.is_empty() {
            msg_success!(Message::ConfigValid(path.display().to_string()));
        }
    }
    if !findings.problems.is_empty() {
        bail!("{}", Message::ConfigProblemCount(findings.problems.len()));
    }
    Ok(())
}

/// A value as a line of text: strings bare, everything else as JSON.
fn shown(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(value) => value.to_string(),
        None => "not set".to_string(),
    }
}

/// What a setting was before an edit, in words.
fn was(before: &Setting) -> String {
    match before.source {
        Source::Set => shown(before.value.as_ref()),
        Source::Default => format!("{} by default", shown(before.value.as_ref())),
        Source::NotSet => "not set".to_string(),
    }
}
