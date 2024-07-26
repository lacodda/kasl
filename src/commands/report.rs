use crate::{
    api::si::Si,
    db::{
        events::{Events, SelectRequest},
        tasks::Tasks,
    },
    libs::{
        config::Config,
        event::{EventGroup, EventType, FormatEvents},
        task::{FormatTasks, Task, TaskFilter},
        view::View,
    },
};
use chrono::{Duration, Local};
use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct ReportArgs {
    #[arg(long, help = "Send report")]
    send: bool,
    #[arg(long, short, help = "Last day report")]
    last: bool,
}

#[tokio::main]
pub async fn cmd(report_args: ReportArgs) -> Result<(), Box<dyn Error>> {
    let mut date = Local::now();
    if report_args.last {
        date = date - Duration::days(1);
    }

    let events = Events::new()?
        .fetch(SelectRequest::Daily, date.date_naive())?
        .merge()
        .update_duration()
        .total_duration()
        .format();
    let mut tasks = Tasks::new()?.fetch(TaskFilter::Date(date.date_naive()))?;

    if report_args.send {
        if tasks.is_empty() {
            println!("Tasks not found((");
            return Ok(());
        }

        let task_chunks: Vec<Vec<Task>> = tasks.divide(events.0.len());

        let events_json = events
            .0
            .iter()
            .enumerate()
            .map(|(index, event)| {
                serde_json::json!({
                    "index": event.id,
                    "from": event.start,
                    "to": event.end,
                    "total_ts": event.duration,
                    "task": task_chunks.get(index).unwrap().to_owned().format(),
                    "data": [],
                    "time": "",
                    "result": ""
                })
            })
            .collect::<Vec<_>>();
        let events_json = serde_json::to_string(&events_json)?;

        match Config::read() {
            Ok(config) => match config.si {
                Some(si_config) => {
                    let mut si = Si::new(&si_config);
                    match si.send(&events_json, &date.date_naive()).await {
                        Ok(status) => {
                            if status.is_success() {
                                let _ = Events::new()?.insert(&EventType::End);
                                println!(
                                    "Your report dated {} has been successfully submitted\nWait for a message to your email address",
                                    date.format("%B %-d, %Y")
                                );
                                if si.is_last_working_day_of_month(&date.date_naive())? {
                                    let monthly_status = si.send_monthly(&date.date_naive()).await?;
                                    if monthly_status.is_success() {
                                        println!(
                                            "Your monthly report dated {} has been successfully submitted\nWait for a message to your email address",
                                            date.format("%B %-d, %Y")
                                        );
                                    }
                                }
                            } else {
                                println!("Status: {}", status);
                            }
                        }
                        Err(e) => eprintln!("Error sending events: {}", e),
                    }
                }
                None => eprintln!("Failed to read SiServer config"),
            },
            Err(e) => eprintln!("Failed to read config: {}", e),
        }

        return Ok(());
    } else {
        println!("\nReport for {}", date.format("%B %-d, %Y"));
        View::events(&events)?;
        if !tasks.is_empty() {
            println!("\nTasks:");
            View::tasks(&tasks)?;
        }
    }

    Ok(())
}
