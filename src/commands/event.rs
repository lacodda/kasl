use crate::{
    db::events::Events,
    libs::{
        event::{EventType, FormatEvents, MergeEvents},
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
}

pub fn cmd(event_args: EventArgs) -> Result<(), Box<dyn Error>> {
    if event_args.show {
        let now = Local::now();
        println!("\nWorking hours for {}", now.format("%B %-d, %Y"));

        let events = Events::new()?.fetch()?.merge().update_duration().total_duration().format();
        View::events(&events)?;

        return Ok(());
    }
    let _ = Events::new()?.insert(&event_args.event_type);

    println!("Time {}", &event_args.event_type);

    Ok(())
}
