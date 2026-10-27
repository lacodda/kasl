//! `kasl plugin`: the plugins installed, what each needs, and running one by hand.
//!
//! A plugin is used because it is installed - there is nothing to enable.
//! What this command adds is seeing them (`list`), telling one what it needs
//! (`setup`), and running one of its commands on a day or an issue (`run`).
//! Events and inbox polls call plugins on their own; see
//! [`crate::libs::plugin`].

use crate::db::inbox::{Inbox, InboxItem};
use crate::libs::config::Config;
use crate::libs::formatter::parse_date;
use crate::libs::json;
use crate::libs::messages::Message;
use crate::libs::output::{self, Format};
use crate::libs::pick;
use crate::libs::plugin::manifest::{Manifest, Setting, Target};
use crate::libs::plugin::{self, Plugin};
use crate::libs::prompt::ensure_interactive;
use crate::libs::report::DayReport;
use crate::libs::secret::Secret;
use crate::{msg_info, msg_print, msg_success};
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use dialoguer::{Input, Password, theme::ColorfulTheme};
use serde_json::{Value, json};

/// Command-line arguments for `kasl plugin`.
#[derive(Debug, Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    command: PluginCommand,
}

#[derive(Debug, Subcommand)]
enum PluginCommand {
    /// Every plugin installed, with what it offers and whether it is set up
    #[command(about = "List the installed plugins")]
    List {
        #[command(flatten)]
        format: Format,
    },

    /// Ask for each setting the plugin declares and keep the answers
    ///
    /// Secret settings go to the OS keyring and are typed without echo; the
    /// rest go to the config file under `plugins.NAME.settings`.
    #[command(about = "Tell a plugin what it needs: a URL, an account, a token")]
    Setup {
        /// The plugin's name: `gitlab` for `kasl-plugin-gitlab`
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// Run one of a plugin's commands on a day or an inbox issue
    #[command(about = "Run a plugin's command")]
    Run {
        /// The plugin's name: `gitlab` for `kasl-plugin-gitlab`
        #[arg(value_name = "NAME")]
        name: String,

        /// The command's key, as `kasl plugin list` shows it
        #[arg(value_name = "COMMAND")]
        command: String,

        /// The issue a command on an issue runs on; asked for on a terminal
        #[arg(value_name = "KEY")]
        key: Option<String>,

        /// The day a command on a day runs on (YYYY-MM-DD or `today`)
        #[arg(long, short, default_value = "today")]
        date: String,

        #[command(flatten)]
        format: Format,
    },
}

pub async fn cmd(args: PluginArgs) -> Result<()> {
    match args.command {
        PluginCommand::List { format } => list(format.json),
        PluginCommand::Setup { name } => setup(&name),
        PluginCommand::Run {
            name,
            command,
            key,
            date,
            format,
        } => run(&name, &command, key, &date, format.json).await,
    }
}

/// The document `kasl plugin list --json` prints.
pub(crate) fn list_document() -> Result<json::PluginList> {
    Ok(json::PluginList {
        directory: plugin::plugins_dir()?.display().to_string(),
        plugins: plugin::discover()?.iter().map(json::PluginShown::from).collect(),
    })
}

fn list(as_json: bool) -> Result<()> {
    let document = list_document()?;
    if as_json {
        return output::print_json(&document);
    }
    if document.plugins.is_empty() {
        msg_info!(Message::PluginsNone(document.directory));
        return Ok(());
    }
    for (index, plugin) in plugin::discover()?.iter().enumerate() {
        if index > 0 {
            msg_print!(String::new());
        }
        for line in describe(plugin) {
            msg_print!(line);
        }
    }
    Ok(())
}

/// One plugin in words: a heading, where it stands, then what it offers.
fn describe(plugin: &Plugin) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(manifest) = &plugin.manifest else {
        lines.push(plugin.name.clone());
        lines.push(format!(
            "  cannot be used: {}",
            plugin.problem.as_deref().unwrap_or("it did not describe itself")
        ));
        lines.push(format!("  at {}", plugin.path.display()));
        return lines;
    };
    lines.push(match &manifest.description {
        Some(about) => format!("{} {} - {}", plugin.name, manifest.version, about),
        None => format!("{} {}", plugin.name, manifest.version),
    });
    lines.push(format!("  {}", standing(plugin)));
    for command in &manifest.commands {
        lines.push(format!(
            "  command  {} (on a {}) - {}",
            command.key,
            command.target.as_str(),
            command.description.as_deref().unwrap_or(&command.label)
        ));
    }
    if !manifest.events.is_empty() {
        let events: Vec<&str> = manifest.events.iter().map(|event| event.as_str()).collect();
        lines.push(format!("  hears    {}", events.join(", ")));
    }
    if manifest.is_source() {
        let requests: Vec<&str> = manifest.requests.iter().map(|request| request.as_str()).collect();
        lines.push(format!("  inbox    a source: {}", requests.join(", ")));
    }
    for setting in &manifest.settings {
        lines.push(format!("  setting  {}", setting_line(setting, plugin)));
    }
    lines.push(format!("  at {}", plugin.path.display()));
    lines
}

fn standing(plugin: &Plugin) -> String {
    if let Some(problem) = &plugin.problem {
        return format!("cannot be used: {problem}");
    }
    if !plugin.missing.is_empty() {
        return format!("needs `kasl plugin setup {}`: {}", plugin.name, plugin.missing.join(", "));
    }
    "ready".to_string()
}

fn setting_line(setting: &Setting, plugin: &Plugin) -> String {
    let mut marks = Vec::new();
    if setting.required {
        marks.push("required");
    }
    if setting.secret {
        marks.push("secret");
    }
    if plugin.missing.contains(&setting.key) {
        marks.push("not set");
    }
    let marks = if marks.is_empty() {
        String::new()
    } else {
        format!(" ({})", marks.join(", "))
    };
    format!("{} - {}{marks}", setting.key, setting.label)
}

/// Asks for every setting the plugin declares and keeps what was typed.
///
/// The current value is offered as the default, so Enter keeps it; a secret
/// is never shown back, and an empty answer keeps the stored one. A value the
/// person clears on purpose - an optional setting left empty - is removed.
fn setup(name: &str) -> Result<()> {
    let plugin = plugin::find(name)?;
    let manifest = plugin.manifest()?.clone();
    if manifest.settings.is_empty() {
        msg_info!(Message::PluginNothingToSetUp(plugin.name));
        return Ok(());
    }
    ensure_interactive(&format!("`kasl plugin setup {}` needs a terminal to ask for its settings", plugin.name))?;

    let mut config = Config::read()?;
    let stored = config.plugins.entry(plugin.name.clone()).or_default();
    for setting in &manifest.settings {
        let prompt = match &setting.description {
            Some(about) => format!("{} ({about})", setting.label),
            None => setting.label.clone(),
        };
        if setting.secret {
            let secret = Secret::new(&plugin::secret_account(&plugin.name, &setting.key), &prompt);
            let has_one = secret.try_get_cached().is_some_and(|value| !value.is_empty());
            let typed = Password::with_theme(&ColorfulTheme::default())
                .with_prompt(if has_one { format!("{prompt} - Enter keeps the stored one") } else { prompt })
                .allow_empty_password(has_one || !setting.required)
                .interact()?;
            if !typed.is_empty() {
                secret.store(&typed)?;
            }
            continue;
        }
        let current = stored.settings.get(&setting.key).and_then(Value::as_str).unwrap_or_default().to_string();
        let typed: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .with_initial_text(current)
            .allow_empty(!setting.required)
            .interact_text()?;
        let typed = typed.trim().to_string();
        if typed.is_empty() {
            stored.settings.remove(&setting.key);
        } else {
            stored.settings.insert(setting.key.clone(), Value::String(typed));
        }
    }
    if config.plugins.get(&plugin.name).is_some_and(|stored| stored.settings.is_empty()) {
        config.plugins.remove(&plugin.name);
    }
    config.save()?;
    msg_success!(Message::PluginSetUp(plugin.name));
    Ok(())
}

/// Runs one command of one plugin on what its target names.
async fn run(name: &str, key: &str, issue: Option<String>, date: &str, as_json: bool) -> Result<()> {
    let plugin = plugin::find(name)?;
    let manifest: Manifest = plugin.ensure_ready()?.clone();
    let Some(command) = manifest.command(key).cloned() else {
        let known: Vec<&str> = manifest.commands.iter().map(|c| c.key.as_str()).collect();
        if known.is_empty() {
            bail!("the plugin `{}` offers no commands", plugin.name);
        }
        bail!("the plugin `{}` has no command `{key}` (it offers: {})", plugin.name, known.join(", "));
    };
    let subject = match command.target {
        Target::Day => {
            if let Some(issue) = issue {
                bail!("`{key}` runs on a day, not on an issue; drop `{issue}` and pass --date if not today");
            }
            serde_json::to_value(DayReport::read(parse_date(date)?)?.to_json())?
        }
        Target::Issue => serde_json::to_value(json::Issue::now(&issue_for(issue)?))?,
    };
    let call = json!({
        "command": command.key,
        "target": command.target.as_str(),
        "subject": subject,
    });
    let limit = command.allowance();
    let invoked = plugin.clone();
    let reply = tokio::task::spawn_blocking(move || plugin::invoke(&invoked, call, limit))
        .await
        .map_err(|error| anyhow::anyhow!("the plugin call stopped: {error}"))??;

    if as_json {
        return output::print_json(&json::PluginRun {
            plugin: plugin.name,
            command: command.key,
            reply,
        });
    }
    match reply.get("message").and_then(Value::as_str) {
        Some(message) => msg_success!(Message::PluginSaid {
            plugin: plugin.name.clone(),
            message: message.to_string(),
        }),
        None => msg_success!(Message::PluginSaid {
            plugin: plugin.name.clone(),
            message: format!("{} done", command.label),
        }),
    }
    // What a plugin found is shown, not kept: kasl has no field a plugin
    // writes to, and every change to kasl's own records goes through kasl.
    if let Some(Value::Object(meta)) = reply.get("meta") {
        for (field, value) in meta {
            let shown = match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            msg_print!(format!("  {field}: {shown}"));
        }
    }
    Ok(())
}

/// The issue a command on an issue runs on: the key given, or one picked.
fn issue_for(key: Option<String>) -> Result<InboxItem> {
    let db = Inbox::new()?;
    let key = match key {
        Some(key) => key,
        None => pick::inbox_issue(&db.list_active(false)?, "Run it on which issue?")?,
    };
    match db.get_by_key(&key)? {
        Some(item) => Ok(item),
        None => bail!("{key} is not in the inbox; `kasl inbox` lists what is"),
    }
}
