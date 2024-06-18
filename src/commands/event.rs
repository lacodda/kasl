use crate::{
    db::events::{Events, SelectRequest},
    libs::{
        event::{EventType, FormatEvents, EventGroup},
        view::View,
    },
};
use chrono::Local;
use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct EventArgs {
    #[arg(
        default_value_t = EventType::Start,
        value_enum
    )]
    pub(crate) event_type: EventType,
    #[arg(short, long)]
    pub(crate) show: bool,
    #[arg(short, long)]
    pub(crate) raw: bool,
}

pub fn cmd(event_args: EventArgs) -> Result<(), Box<dyn Error>> {
    let now = Local::now();
    if event_args.raw {
        println!("\nRaw events for {}", now.format("%B %-d, %Y"));

        let events = Events::new()?.fetch(SelectRequest::Daily, now.date_naive())?.format();
        View::events_raw(&events)?;

        return Ok(());
    } else if event_args.show {
        println!("\nWorking hours for {}", now.format("%B %-d, %Y"));

        let events = Events::new()?.fetch(SelectRequest::Daily, now.date_naive())?.merge().update_duration().total_duration().format();
        View::events(&events)?;

        return Ok(());
    }
    let _ = Events::new()?.insert(&event_args.event_type);

    println!("Time {}", &event_args.event_type);

    Ok(())
}
