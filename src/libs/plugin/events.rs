//! Telling plugins what happened: `report_ready`, `workday_end`, `new_inbox_issue`.
//!
//! An event goes to every plugin that is set up and lists it in its
//! manifest, one after another, each with its own time limit. The moment it
//! describes has already happened when the plugins hear of it - the day is
//! closed, the issue is in the inbox - so a plugin that fails changes nothing
//! in kasl: it is said out loud, with the plugin's own reason, and the rest
//! still hear.
//!
//! Delivered once, at the moment, and not queued: a plugin that was down
//! misses it. Keeping what a channel missed and offering it again is one
//! queue for every channel - kasl-server's days, SiServer's reports and the
//! plugins' events - and is the plan's own stage, not three copies of it.

use super::manifest::{self, Event};
use super::{Plugin, discover, invoke};
use crate::libs::messages::Message;
use crate::{msg_info, msg_warning};
use serde_json::{Value, json};
use tracing::debug;

/// What one plugin made of an event.
#[derive(Debug)]
pub struct Delivery {
    /// The plugin's name.
    pub plugin: String,
    /// The line it answered with, if any; or why it failed.
    pub outcome: Result<Option<String>, String>,
}

/// The plugins that would hear `event` now.
///
/// Read by `kasl report --send --show`, which owes the person the whole of
/// where their day goes before it goes there.
pub fn listeners(event: Event) -> Vec<Plugin> {
    match discover() {
        Ok(plugins) => plugins
            .into_iter()
            .filter(|plugin| plugin.ready() && plugin.manifest.as_ref().is_some_and(|m| m.hears(event)))
            .collect(),
        Err(error) => {
            debug!("plugins: could not look for listeners of {}: {error:#}", event.as_str());
            Vec::new()
        }
    }
}

/// Tells every listener of `event` about `subject`, and waits for each.
///
/// Blocks: call it from a thread that may wait, `tokio::task::spawn_blocking`
/// in async code.
pub fn fire(event: Event, subject: &Value) -> Vec<Delivery> {
    fire_to(&listeners(event), event, subject)
}

/// [`fire`] to listeners found earlier - by a caller that decided by them
/// whether there is anywhere to send at all.
pub fn fire_to(plugins: &[Plugin], event: Event, subject: &Value) -> Vec<Delivery> {
    plugins
        .iter()
        .map(|plugin| Delivery {
            plugin: plugin.name.clone(),
            outcome: deliver(plugin, event, subject),
        })
        .collect()
}

fn deliver(plugin: &Plugin, event: Event, subject: &Value) -> Result<Option<String>, String> {
    let call = json!({ "event": event.as_str(), "subject": subject });
    invoke(plugin, call, manifest::allowance(None))
        .map(|reply| reply.get("message").and_then(Value::as_str).map(str::to_string))
        .map_err(|error| format!("{error:#}"))
}

/// Says what the listeners answered: their lines as they are, their failures as warnings.
pub fn announce(event: Event, deliveries: &[Delivery]) {
    for delivery in deliveries {
        match &delivery.outcome {
            Ok(Some(message)) => msg_info!(Message::PluginSaid {
                plugin: delivery.plugin.clone(),
                message: message.clone(),
            }),
            Ok(None) => {}
            Err(error) => msg_warning!(Message::PluginEventFailed {
                event: event.as_str().to_string(),
                error: error.clone(),
            }),
        }
    }
}

/// [`fire`] and [`announce`] in one, for a caller that only reports.
pub fn tell(event: Event, subject: &Value) {
    let deliveries = fire(event, subject);
    announce(event, &deliveries);
}

/// [`tell`] from async code, off the runtime's own threads.
pub async fn tell_async(event: Event, subject: Value) {
    match tokio::task::spawn_blocking(move || tell(event, &subject)).await {
        Ok(()) => {}
        Err(error) => debug!("plugins: delivering {} stopped: {error}", event.as_str()),
    }
}
