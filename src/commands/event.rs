use crate::{
    db::events::Events,
    libs::{
        event::{EventType, MergeEvents},
        view::View,
    },
};
use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct EventArgs {
    #[arg(
        default_value_t = EventType::Start,
        value_enum
    )]
    event_type: EventType,
    #[arg(short, long)]
    show: bool,
}

pub fn cmd(event_args: EventArgs) -> Result<(), Box<dyn Error>> {
    if event_args.show {
        let events = Events::new()?.fetch()?.merge();
        View::events(&events)?;

        return Ok(());
    }
    let _ = Events::new()?.insert(&event_args.event_type);

    println!("Time {}", &event_args.event_type);

    Ok(())
}
