//! The interactive setup wizard behind `kasl setup`.
//!
//! Every prompt lives here, away from the data model: the module list,
//! the per-module parameter prompts, and the two list editors (discovery
//! ignore names, Jira inbox fields).

use super::{Config, ConfigModule, JiraCustomField, JiraInboxConfig, MonitorConfig, ProductivityConfig, ReportConfig, ServerConfig, TaskDiscoveryConfig};
use crate::api::gitlab::GitLabConfig;
use crate::api::jira::JiraConfig;
use crate::api::si::SiConfig;
use crate::libs::messages::Message;
use crate::libs::task::normalize_task_name;
use crate::{msg_info, msg_print, msg_success};
use anyhow::Result;
use dialoguer::{Confirm, Input, MultiSelect, theme::ColorfulTheme};

impl Config {
    /// Runs the setup wizard: pick modules, prompt each one's settings,
    /// return the updated config (the caller saves it).
    ///
    /// ```rust,no_run
    /// # fn main() -> anyhow::Result<()> {
    /// use kasl::libs::config::Config;
    ///
    /// let config = Config::init()?;
    /// config.save()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn init() -> Result<Self> {
        // The whole wizard is prompts: every module below asks for values, and
        // several read secrets. Without a terminal there is nobody to answer.
        crate::libs::prompt::ensure_interactive("`kasl setup` is an interactive wizard and needs a terminal")?;

        // Existing values become the wizard's defaults.
        let mut config = Self::read().unwrap_or_default();

        let node_descriptions = [
            SiConfig::module(),
            GitLabConfig::module(),
            JiraConfig::module(),
            ConfigModule {
                key: "monitor".to_string(),
                name: "Monitor".to_string(),
            },
            ConfigModule {
                key: "server".to_string(),
                name: "Server".to_string(),
            },
            ConfigModule {
                key: "productivity".to_string(),
                name: "Productivity".to_string(),
            },
            ConfigModule {
                key: "report".to_string(),
                name: "Report".to_string(),
            },
            ConfigModule {
                key: "task_discovery".to_string(),
                name: "Task discovery".to_string(),
            },
            ConfigModule {
                key: "jira_inbox".to_string(),
                name: "Jira inbox".to_string(),
            },
        ];

        let selected_nodes = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptSelectModules.to_string())
            .items(node_descriptions.iter().map(|module| &module.name).collect::<Vec<_>>())
            .interact()?;

        for &selection in &selected_nodes {
            match node_descriptions[selection].key.as_str() {
                // External API integrations delegate to their own setup methods
                "si" => config.si = Some(SiConfig::init(&config.si)?),
                "gitlab" => config.gitlab = Some(GitLabConfig::init(&config.gitlab)?),
                "jira" => config.jira = Some(JiraConfig::init(&config.jira)?),

                "monitor" => {
                    let default = config.monitor.clone().unwrap_or_default();
                    msg_print!(Message::ConfigModuleMonitor);
                    config.monitor = Some(MonitorConfig {
                        min_pause_duration: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptMinPauseDuration.to_string())
                            .default(default.min_pause_duration)
                            .interact_text()?,

                        pause_threshold: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptPauseThreshold.to_string())
                            .default(default.pause_threshold)
                            .interact_text()?,

                        poll_interval: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptPollInterval.to_string())
                            .default(default.poll_interval)
                            .interact_text()?,

                        activity_threshold: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptActivityThreshold.to_string())
                            .default(default.activity_threshold)
                            .interact_text()?,

                        min_work_interval: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptMinWorkInterval.to_string())
                            .default(default.min_work_interval)
                            .interact_text()?,

                        // Preserve the pause-merge gap (edited manually in config.json)
                        pause_merge_gap: default.pause_merge_gap,
                    });
                }

                "server" => {
                    let default = config.server.clone().unwrap_or(ServerConfig {
                        api_url: "".to_string(),
                        auth_token: "".to_string(),
                    });
                    msg_print!(Message::ConfigModuleServer);
                    config.server = Some(ServerConfig {
                        api_url: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptServerApiUrl.to_string())
                            .default(default.api_url)
                            .interact_text()?,

                        auth_token: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptServerAuthToken.to_string())
                            .default(default.auth_token)
                            .interact_text()?,
                    });
                }

                "productivity" => {
                    let default = config.productivity.clone().unwrap_or_default();
                    msg_print!(Message::ConfigModuleProductivity);
                    config.productivity = Some(ProductivityConfig {
                        min_productivity_threshold: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptMinProductivityThreshold.to_string())
                            .default(default.min_productivity_threshold)
                            .interact_text()?,

                        workday_hours: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptWorkdayHours.to_string())
                            .default(default.workday_hours)
                            .interact_text()?,

                        min_workday_fraction_before_suggest: Input::with_theme(&ColorfulTheme::default())
                            .with_prompt(Message::PromptMinWorkdayFraction.to_string())
                            .default(default.min_workday_fraction_before_suggest)
                            .interact_text()?,
                    });
                }

                "report" => {
                    let default = config.report.clone().unwrap_or(ReportConfig {
                        output_dir: None,
                        filename_template: None,
                        language: None,
                        template: None,
                    });
                    let output_dir: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Reports output directory")
                        .default(default.output_dir.unwrap_or_default())
                        .allow_empty(true)
                        .interact_text()?;
                    let filename_template: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Report file name template (placeholders: {date}, {seq})")
                        .default(default.filename_template.unwrap_or_else(|| "daily_report_{date}{seq}".to_string()))
                        .allow_empty(true)
                        .interact_text()?;
                    let language: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Report language (ru, en)")
                        .default(default.language.unwrap_or_else(|| "ru".to_string()))
                        .allow_empty(true)
                        .interact_text()?;
                    let template: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("Report design template name")
                        .default(default.template.unwrap_or_else(|| "siserver".to_string()))
                        .allow_empty(true)
                        .interact_text()?;
                    config.report = Some(ReportConfig {
                        output_dir: if output_dir.trim().is_empty() { None } else { Some(output_dir) },
                        filename_template: if filename_template.trim().is_empty() { None } else { Some(filename_template) },
                        language: if language.trim().is_empty() { None } else { Some(language) },
                        template: if template.trim().is_empty() { None } else { Some(template) },
                    });
                }

                "task_discovery" => {
                    config.task_discovery = Some(configure_task_discovery(config.task_discovery.clone().unwrap_or_default())?);
                }

                "jira_inbox" => {
                    config.jira_inbox = Some(configure_jira_inbox(config.jira_inbox.clone().unwrap_or_default())?);
                }

                _ => {} // Unknown module keys are safely ignored
            }
        }

        Ok(config)
    }
}

/// Interactive editor for the task discovery ignore list.
fn configure_task_discovery(mut discovery: TaskDiscoveryConfig) -> Result<TaskDiscoveryConfig> {
    msg_print!(Message::ConfigModuleTaskDiscovery, true);

    if discovery.ignore_names.is_empty() {
        msg_info!(Message::TaskDiscoveryIgnoreListEmpty);
    } else {
        msg_print!(Message::TaskDiscoveryIgnoreListHeader, true);
        for (idx, name) in discovery.ignore_names.iter().enumerate() {
            println!("  {}. {}", idx + 1, name);
        }
        println!();

        let remove = MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptSelectIgnoreNamesToRemove.to_string())
            .items(&discovery.ignore_names)
            .interact()?;

        if !remove.is_empty() {
            let mut keep = Vec::new();
            for (idx, name) in discovery.ignore_names.iter().enumerate() {
                if !remove.contains(&idx) {
                    keep.push(name.clone());
                }
            }
            discovery.ignore_names = keep;
        }
    }

    loop {
        let name: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptAddIgnoreName.to_string())
            .allow_empty(true)
            .interact_text()?;

        let trimmed = name.trim();
        if trimmed.is_empty() {
            break;
        }

        let key = normalize_task_name(trimmed);
        let exists = discovery.ignore_names.iter().any(|existing| normalize_task_name(existing) == key);
        if exists {
            msg_info!(Message::TaskDiscoveryIgnoreNameExists(trimmed.to_string()));
        } else {
            discovery.ignore_names.push(trimmed.to_string());
            msg_success!(Message::TaskDiscoveryIgnoreNameAdded(trimmed.to_string()));
        }
    }

    Ok(discovery)
}

/// Interactive wizard for Jira inbox polling settings.
fn configure_jira_inbox(default: JiraInboxConfig) -> Result<JiraInboxConfig> {
    msg_print!(Message::ConfigModuleJiraInbox, true);

    let enabled = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptJiraInboxEnabled.to_string())
        .default(default.enabled)
        .interact()?;

    let poll_interval_secs: u64 = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptJiraInboxPollInterval.to_string())
        .default(default.poll_interval_secs)
        .interact_text()?;

    let notify = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptJiraInboxNotify.to_string())
        .default(default.notify)
        .interact()?;

    let default_sort_id = default.sort_by_field.clone().unwrap_or_default();
    let sort_field_id: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(Message::PromptJiraInboxSortFieldId.to_string())
        .with_initial_text(&default_sort_id)
        .allow_empty(true)
        .interact_text()?;

    let mut custom_fields = Vec::new();
    let mut sort_by_field = None;

    let sort_trimmed = sort_field_id.trim();
    if !sort_trimmed.is_empty() {
        let default_label = default
            .custom_fields
            .iter()
            .find(|f| f.id == sort_trimmed)
            .map(|f| f.label.clone())
            .unwrap_or_else(|| "Scoring".to_string());

        let sort_label: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptJiraInboxSortFieldLabel.to_string())
            .default(default_label)
            .interact_text()?;

        let label = {
            let t = sort_label.trim();
            if t.is_empty() { "Scoring".to_string() } else { t.to_string() }
        };
        custom_fields.push(JiraCustomField {
            id: sort_trimmed.to_string(),
            label,
        });
        sort_by_field = Some(sort_trimmed.to_string());
    }

    loop {
        let extra_id: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptJiraInboxExtraFieldId.to_string())
            .allow_empty(true)
            .interact_text()?;
        let trimmed = extra_id.trim();
        if trimmed.is_empty() {
            break;
        }
        if custom_fields.iter().any(|f| f.id == trimmed) {
            continue;
        }
        let extra_label: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(Message::PromptJiraInboxExtraFieldLabel.to_string())
            .default(trimmed.to_string())
            .interact_text()?;
        custom_fields.push(JiraCustomField {
            id: trimmed.to_string(),
            label: {
                let t = extra_label.trim();
                if t.is_empty() { trimmed.to_string() } else { t.to_string() }
            },
        });
    }

    Ok(JiraInboxConfig {
        enabled,
        poll_interval_secs: poll_interval_secs.max(30),
        notify,
        // Change/gone toasts keep their previous (or default) values; the
        // wizard stays short and these are tuned via the config file.
        notify_changes: default.notify_changes,
        notify_gone: default.notify_gone,
        custom_fields,
        sort_by_field,
    })
}
