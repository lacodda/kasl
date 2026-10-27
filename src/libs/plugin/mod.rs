//! Plugins: programs named `kasl-plugin-*` that kasl runs for a moment.
//!
//! The line's convention, the same one kilna speaks: a plugin is an ordinary
//! executable found by its name, asked to describe itself with `--manifest`
//! and run with `run`, one JSON object on its stdin and one on its stdout. It
//! lives for the duration of a call; nothing here starts a service. No
//! dynamic libraries - Rust has no stable ABI for them - and no sandboxed
//! runtime, because the integrations plugins exist for (a mail client, a
//! company's own tracker) are exactly what a sandbox forbids. ADR 0009 has
//! the reasoning.
//!
//! kasl calls a plugin three ways, and an invocation names which one it is:
//!
//! * a **command** a person runs by hand (`kasl plugin run NAME KEY`), with
//!   the day or the issue it is run on as its subject;
//! * an **event** the plugin listens to (`report_ready`, `workday_end`,
//!   `new_inbox_issue`) - see [`events`];
//! * a **request** to an inbox source (`list_issues`, `transition`,
//!   `comment`) - see [`crate::libs::inbox_source`].
//!
//! Every invocation also carries the plugin's settings, so a plugin never has
//! to read kasl's config or its keyring on its own.

pub mod events;
pub mod manifest;

use crate::db::inbox::JIRA_SOURCE;
use crate::libs::config::Config;
use crate::libs::data_storage::DataStorage;
use crate::libs::secret::Secret;
use anyhow::{Context, Result, anyhow, bail};
use manifest::{Manifest, PREFIX, Setting};
#[cfg(unix)]
use process_wrap::std::ProcessGroup;
use process_wrap::std::{ChildWrapper, CommandWrap};
#[cfg(windows)]
use process_wrap::std::{CreationFlags, JobObject};
use serde_json::{Map, Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

/// A plugin found on this machine, whether or not it can be used.
#[derive(Debug, Clone)]
pub struct Plugin {
    /// What kasl calls it: the executable's name without the prefix, `gitlab`.
    ///
    /// The executable, not the manifest, names a plugin: it is what the
    /// person installed and what they type, and two plugins cannot share it.
    pub name: String,
    /// The executable's name, `kasl-plugin-gitlab`.
    pub executable: String,
    pub path: PathBuf,
    /// `None` when the manifest could not be read; `problem` says why.
    pub manifest: Option<Manifest>,
    /// Why the plugin cannot be used at all.
    pub problem: Option<String>,
    /// Required settings with no value yet, by key.
    pub missing: Vec<String>,
}

impl Plugin {
    /// Whether kasl can talk to it.
    pub fn usable(&self) -> bool {
        self.manifest.is_some() && self.problem.is_none()
    }

    /// Whether it can be called now: usable, and told everything it requires.
    pub fn ready(&self) -> bool {
        self.usable() && self.missing.is_empty()
    }

    /// Its manifest, or why there is none to use.
    pub fn manifest(&self) -> Result<&Manifest> {
        match (&self.manifest, &self.problem) {
            (Some(manifest), None) => Ok(manifest),
            (_, Some(problem)) => bail!("the plugin `{}` cannot be used: {problem}", self.name),
            (None, None) => bail!("the plugin `{}` did not describe itself", self.name),
        }
    }

    /// Refuses a call to a plugin that is not set up, naming what it lacks.
    pub fn ensure_ready(&self) -> Result<&Manifest> {
        let manifest = self.manifest()?;
        if !self.missing.is_empty() {
            bail!(
                "the plugin `{}` is not set up: it needs {}; run `kasl plugin setup {}`",
                self.name,
                self.missing.iter().map(|key| format!("`{key}`")).collect::<Vec<_>>().join(", "),
                self.name
            );
        }
        Ok(manifest)
    }
}

/// Where kasl keeps plugins of its own: `plugins` in its data directory.
pub fn plugins_dir() -> Result<PathBuf> {
    DataStorage::new().get_path("plugins")
}

/// Where kasl looks for plugins, in order: its own directory, then `PATH`.
pub fn search_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = plugins_dir().into_iter().collect();
    if let Some(system) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&system));
    }
    paths
}

/// Every plugin installed, usable or not, sorted by name.
///
/// A plugin that fails to describe itself is still listed, with the reason:
/// an integration that silently does not appear is indistinguishable from
/// one that was never installed.
pub fn discover() -> Result<Vec<Plugin>> {
    let config = Config::read()?;
    let mut found: Vec<Plugin> = Vec::new();
    for directory in search_paths() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();
            let Some(executable) = executable_name(&path) else {
                continue;
            };
            let Some(name) = executable.strip_prefix(PREFIX).filter(|name| !name.is_empty()) else {
                continue;
            };
            // The first copy on the path wins, as with any other command.
            if found.iter().any(|plugin| plugin.executable == executable) {
                continue;
            }
            let mut plugin = describe(&path, name, &executable);
            judge(&mut plugin, &config, &|account| Secret::new(account, "").try_get_cached());
            found.push(plugin);
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// The plugin named `name`, with or without its prefix.
pub fn find(name: &str) -> Result<Plugin> {
    let wanted = name.strip_prefix(PREFIX).unwrap_or(name);
    let plugins = discover()?;
    if let Some(plugin) = plugins.iter().find(|plugin| plugin.name == wanted) {
        return Ok(plugin.clone());
    }
    let known: Vec<&str> = plugins.iter().map(|plugin| plugin.name.as_str()).collect();
    if known.is_empty() {
        bail!(
            "no plugin `{wanted}`: none is installed (kasl looks for `{PREFIX}*` in {} and on your PATH)",
            plugins_dir().map(|dir| dir.display().to_string()).unwrap_or_default()
        );
    }
    bail!("no plugin `{wanted}` (installed: {})", known.join(", "))
}

/// How long a plugin may take to describe itself.
///
/// Every discovery asks every plugin, and discovery runs on each poll and
/// each `kasl end`; a manifest is printed, not computed, so a plugin that
/// takes longer than this is stuck and is listed as such.
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Asks a plugin what it is.
fn describe(path: &Path, name: &str, executable: &str) -> Plugin {
    let base = Plugin {
        name: name.to_string(),
        executable: executable.to_string(),
        path: path.to_path_buf(),
        manifest: None,
        problem: None,
        missing: Vec::new(),
    };
    let output = match run_child(path, "--manifest", None, MANIFEST_TIMEOUT) {
        Ok(output) => output,
        Err(error) => {
            return Plugin {
                problem: Some(format!("{error:#}")),
                ..base
            };
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Plugin {
            problem: Some(if stderr.trim().is_empty() {
                "it refused to describe itself".to_string()
            } else {
                stderr.trim().to_string()
            }),
            ..base
        };
    }
    let manifest: Manifest = match serde_json::from_slice(&output.stdout) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Plugin {
                problem: Some(format!("its manifest could not be read: {error}")),
                ..base
            };
        }
    };
    Plugin {
        problem: manifest.problem(),
        manifest: Some(manifest),
        ..base
    }
}

/// What the config and the keyring say about a plugin that described itself.
///
/// `secret` reads one keyring account; it is passed in so the rule can be
/// tested without touching the machine's real keyring.
fn judge(plugin: &mut Plugin, config: &Config, secret: &dyn Fn(&str) -> Option<String>) {
    let Some(manifest) = &plugin.manifest else {
        return;
    };
    if plugin.problem.is_none() && plugin.name == JIRA_SOURCE && manifest.is_source() && config.jira.is_some() {
        plugin.problem = Some(
            "kasl's own Jira source is configured and owns the name `jira`; remove the `jira` block from the config to let this plugin take its place"
                .to_string(),
        );
    }
    let values = values_of(&plugin.name, &manifest.settings, config, secret);
    plugin.missing = manifest
        .settings
        .iter()
        .filter(|setting| setting.required && !values.contains_key(&setting.key))
        .map(|setting| setting.key.clone())
        .collect();
}

/// The keyring account a secret setting is kept under.
///
/// `plugin.gitlab.token`: readable in the OS's own credential manager, and
/// the plugin's name in it, so removing a plugin's secrets by hand is
/// possible without kasl.
pub fn secret_account(plugin: &str, key: &str) -> String {
    format!("plugin.{plugin}.{key}")
}

/// The values a plugin's declared settings have now.
///
/// Only declared settings, and only those with a value: a key the config
/// holds that the manifest no longer declares is not handed over, and a
/// setting never set is absent rather than null.
fn values_of(name: &str, settings: &[Setting], config: &Config, secret: &dyn Fn(&str) -> Option<String>) -> Map<String, Value> {
    let stored = config.plugins.get(name).map(|plugin| &plugin.settings);
    let mut values = Map::new();
    for setting in settings {
        let value = if setting.secret {
            secret(&secret_account(name, &setting.key)).filter(|value| !value.is_empty()).map(Value::String)
        } else {
            stored.and_then(|stored| stored.get(&setting.key)).filter(|value| !is_blank(value)).cloned()
        };
        if let Some(value) = value {
            values.insert(setting.key.clone(), value);
        }
    }
    values
}

fn is_blank(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        _ => false,
    }
}

/// Runs one call of one plugin and returns what it answered.
///
/// `call` is the invocation without the parts every call shares - the
/// protocol version and the settings, added here. The answer is the JSON
/// object the plugin printed; silence is an empty object, success with
/// nothing to say. A non-zero exit, a reply that is not an object, and a
/// reply that sets `error` are all failures, each with its reason.
pub fn invoke(plugin: &Plugin, call: Value, limit: Duration) -> Result<Map<String, Value>> {
    let manifest = plugin.manifest()?;
    let config = Config::read()?;
    let settings = values_of(&plugin.name, &manifest.settings, &config, &|account| Secret::new(account, "").try_get_cached());
    let mut invocation = Map::new();
    invocation.insert("protocol_version".into(), json!(manifest::PROTOCOL_VERSION));
    if let Value::Object(fields) = call {
        invocation.extend(fields);
    }
    invocation.insert("settings".into(), Value::Object(settings));
    let mut payload = serde_json::to_vec(&Value::Object(invocation))?;
    payload.push(b'\n');

    let output = run_child(&plugin.path, "run", Some(payload), limit).map_err(|error| anyhow!("the plugin `{}` {error:#}", plugin.name))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() {
            format!("failed with {}", output.status)
        } else {
            stderr.trim().to_string()
        };
        bail!("the plugin `{}` failed: {detail}", plugin.name);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let reply = parse_reply(stdout.trim()).map_err(|why| anyhow!("the plugin `{}` {why}", plugin.name))?;
    if let Some(error) = reply.get("error").and_then(Value::as_str) {
        bail!("the plugin `{}` says: {error}", plugin.name);
    }
    Ok(reply)
}

/// Reads a plugin's stdout as its reply.
fn parse_reply(stdout: &str) -> std::result::Result<Map<String, Value>, String> {
    if stdout.is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(stdout) {
        Ok(Value::Object(reply)) => Ok(reply),
        Ok(_) => Err(format!("answered with something other than a JSON object: {stdout}")),
        Err(error) => Err(format!("answered with what is not JSON ({error}): {stdout}")),
    }
}

/// Runs the executable with one argument, feeds it `input`, and waits for it
/// within `limit`.
///
/// The child is started in a group of its own - a job object on Windows, a
/// process group elsewhere - so stopping it at the limit stops whatever it
/// started too: a plugin written as a `.cmd` that runs PowerShell is two
/// processes, and killing only the first would leave the second running.
///
/// Its output is read on threads of its own: a child that writes more than a
/// pipe holds blocks until someone reads, and waiting for it to exit first
/// would wait for ever. Its input is written on one too, for the mirror case.
fn run_child(path: &Path, argument: &str, input: Option<Vec<u8>>, limit: Duration) -> Result<std::process::Output> {
    let mut command = std::process::Command::new(path);
    command
        .arg(argument)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = grouped(command).spawn().context("could not be started")?;

    let writer = match (child.stdin().take(), input) {
        (Some(mut stdin), Some(bytes)) => Some(std::thread::spawn(move || {
            // A plugin may answer without reading its input; if it exits
            // first, the write sees a closed pipe, and the verdict still
            // arrives through its stdout and exit status.
            match stdin.write_all(&bytes) {
                Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe => Err(error),
                _ => Ok(()),
            }
        })),
        _ => None,
    };
    let mut stdout = child.stdout().take();
    let read_out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(pipe) = stdout.as_mut() {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    });
    let mut stderr = child.stderr().take();
    let read_err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(pipe) = stderr.as_mut() {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    });

    let status = wait_within(&mut child, limit)?;
    if let Some(writer) = writer
        && let Ok(Err(error)) = writer.join()
    {
        bail!("could not be handed its request: {error}");
    }
    Ok(std::process::Output {
        status,
        stdout: read_out.join().unwrap_or_default(),
        stderr: read_err.join().unwrap_or_default(),
    })
}

/// Waits for a child, and stops it - with everything it started - at `limit`.
fn wait_within(child: &mut Box<dyn ChildWrapper>, limit: Duration) -> Result<std::process::ExitStatus> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                bail!("did not answer within {} seconds and was stopped", limit.as_secs());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => bail!("could not be waited on: {error}"),
        }
    }
}

/// The command, wrapped to start in a group of its own and without a window.
fn grouped(command: std::process::Command) -> CommandWrap {
    let mut wrapped = CommandWrap::from(command);
    // The job object replaces the command's creation flags with its own, so
    // the flag that keeps a console window from flashing - from the watcher,
    // which has no console to lend - is handed to it.
    #[cfg(windows)]
    {
        wrapped.wrap(CreationFlags(windows::Win32::System::Threading::CREATE_NO_WINDOW));
        wrapped.wrap(JobObject);
    }
    #[cfg(unix)]
    wrapped.wrap(ProcessGroup::leader());
    wrapped
}

/// The name a plugin is known by: the file name without its extension.
fn executable_name(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let name = path.file_stem()?.to_str()?;
    // On Windows only these are runnable; without the check, a stray
    // `kasl-plugin-notes.txt` would be treated as a program.
    #[cfg(windows)]
    {
        let runnable = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "exe" | "cmd" | "bat"));
        if !runnable {
            return None;
        }
    }
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libs::config::PluginConfig;

    fn manifest(raw: &str) -> Manifest {
        serde_json::from_str(raw).unwrap()
    }

    fn plugin(name: &str, raw: &str) -> Plugin {
        let manifest = manifest(raw);
        Plugin {
            name: name.to_string(),
            executable: format!("{PREFIX}{name}"),
            path: PathBuf::from(name),
            problem: manifest.problem(),
            manifest: Some(manifest),
            missing: Vec::new(),
        }
    }

    const NEEDS: &str = r#"{"protocol_version":1,"name":"g","version":"1",
        "settings":[{"key":"url","label":"URL","required":true},
                    {"key":"token","label":"Token","secret":true,"required":true},
                    {"key":"label","label":"Label"}]}"#;

    #[test]
    fn a_required_setting_without_a_value_is_missing() {
        let mut gitlab = plugin("gitlab", NEEDS);
        let config = Config::default();

        judge(&mut gitlab, &config, &|_| None);

        assert_eq!(gitlab.missing, vec!["url".to_string(), "token".to_string()]);
        assert!(gitlab.usable() && !gitlab.ready());
        let refusal = gitlab.ensure_ready().unwrap_err().to_string();
        assert!(refusal.contains("`url`, `token`") && refusal.contains("kasl plugin setup gitlab"), "{refusal}");
    }

    #[test]
    fn a_secret_comes_from_the_keyring_and_the_rest_from_the_config() {
        let mut gitlab = plugin("gitlab", NEEDS);
        let mut config = Config::default();
        let mut stored = Map::new();
        stored.insert("url".into(), json!("https://gitlab.example.com"));
        stored.insert("token".into(), json!("not-here"));
        stored.insert("stale".into(), json!("no longer declared"));
        config.plugins.insert("gitlab".into(), PluginConfig { settings: stored });
        let keyring = |account: &str| (account == "plugin.gitlab.token").then(|| "s3cret".to_string());

        judge(&mut gitlab, &config, &keyring);
        let values = values_of("gitlab", &gitlab.manifest.as_ref().unwrap().settings, &config, &keyring);

        assert!(gitlab.ready());
        assert_eq!(values.get("token"), Some(&json!("s3cret")), "a secret is never read from the config file");
        assert_eq!(values.get("url"), Some(&json!("https://gitlab.example.com")));
        assert!(!values.contains_key("stale"), "only what the manifest declares is handed over");
        assert!(!values.contains_key("label"), "an unset setting is absent, not null");
    }

    #[test]
    fn a_blank_value_does_not_count_as_set() {
        let mut gitlab = plugin("gitlab", NEEDS);
        let mut config = Config::default();
        let mut stored = Map::new();
        stored.insert("url".into(), json!("  "));
        config.plugins.insert("gitlab".into(), PluginConfig { settings: stored });

        judge(&mut gitlab, &config, &|_| Some(String::new()));

        assert_eq!(gitlab.missing, vec!["url".to_string(), "token".to_string()]);
    }

    #[test]
    fn a_plugin_source_named_jira_gives_way_to_the_configured_jira() {
        let source = r#"{"protocol_version":1,"name":"jira","version":"1","requests":["list_issues"]}"#;
        let mut config = Config::default();
        let mut alone = plugin("jira", source);
        judge(&mut alone, &config, &|_| None);
        assert!(alone.usable(), "with no Jira configured the name is free");

        config.jira = Some(serde_json::from_value(json!({"login": "me", "api_url": "https://jira.example.com"})).unwrap());
        let mut beside = plugin("jira", source);
        judge(&mut beside, &config, &|_| None);
        assert!(beside.problem.unwrap().contains("owns the name `jira`"));
    }

    #[test]
    fn silence_is_an_empty_reply_and_nonsense_is_refused() {
        assert_eq!(parse_reply("").unwrap(), Map::new());
        assert!(parse_reply("[1, 2]").unwrap_err().contains("other than a JSON object"));
        assert!(parse_reply("done!").unwrap_err().contains("not JSON"));
        assert_eq!(parse_reply(r#"{"message":"ok"}"#).unwrap().get("message"), Some(&json!("ok")));
    }

    #[test]
    fn the_secret_account_names_the_plugin() {
        assert_eq!(secret_account("gitlab", "token"), "plugin.gitlab.token");
    }

    /// A child that runs for a while and says nothing.
    fn a_sleeper() -> std::process::Command {
        #[cfg(windows)]
        {
            let mut command = std::process::Command::new("cmd");
            command.args(["/C", "ping -n 6 127.0.0.1 >NUL"]);
            command
        }
        #[cfg(not(windows))]
        {
            let mut command = std::process::Command::new("sleep");
            command.arg("5");
            command
        }
    }

    #[test]
    fn a_child_that_does_not_answer_is_stopped_at_its_limit() {
        let mut command = a_sleeper();
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = grouped(command).spawn().unwrap();
        let started = Instant::now();

        let refused = wait_within(&mut child, Duration::from_millis(300)).unwrap_err().to_string();

        assert!(refused.contains("did not answer within"), "{refused}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "stopped at the limit, not when it finished on its own"
        );
    }
}
