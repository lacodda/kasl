use crate::{
    db::{events::Events, tasks::Tasks},
    libs::{
        config::Config,
        event::{FormatEvents, MergeEvents},
        http::Http,
        task::{FormatTasks, TaskFilter},
        view::View,
    },
};
use chrono::Local;
use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct ReportArgs {
    #[arg(long, help = "Send report")]
    send: bool,
}

#[tokio::main]
pub async fn cmd(report_args: ReportArgs) -> Result<(), Box<dyn Error>> {
    let events = Events::new()?.fetch()?.merge().update_duration().total_duration().format();
    let mut tasks = Tasks::new()?.fetch(TaskFilter::Today)?;

    if report_args.send {
        if tasks.is_empty() {
            println!("Tasks not found((");
            return Ok(());
        }

        let events_json = events
            .0
            .iter()
            .map(|event| {
                let mut task = "".to_string();
                if event.id == 1 {
                    task = tasks.format();
                }
                serde_json::json!({
                    "index": event.id,
                    "from": event.start,
                    "to": event.end,
                    "total_ts": event.duration,
                    "task": task,
                    "data": [],
                    "time": "",
                    "result": ""
                })
            })
            .collect::<Vec<_>>();
        let events_json = serde_json::to_string(&events_json)?;

        match Config::read() {
            Ok(config) => match Http::new().send(&config.url, &config.session_id, events_json).await {
                Ok(status) => {
                    if status == 200 {
                        let date = Local::now().format("%B %-d, %Y").to_string();
                        println!("Your report dated {} has been successfully submitted\nWait for a message to your email address", date);
                    } else {
                        println!("Status: {}", status);
                    }
                }
                Err(e) => eprintln!("Error sending events: {}", e),
            },
            Err(e) => eprintln!("Failed to read config: {}", e),
        }

        return Ok(());
    } else {
        let now = Local::now();
        println!("\nReport for {}", now.format("%B %-d, %Y"));
        View::events(&events)?;
        if !tasks.is_empty() {
            println!("\nTasks:");
            View::tasks(&tasks)?;
        }
    }

    Ok(())
}
