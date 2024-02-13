use crate::{
    db::events::Events,
    libs::event::{Event, EventType},
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
}

pub fn cmd(event_args: EventArgs) -> Result<(), Box<dyn Error>> {
    let event = Event::new(&event_args.event_type);
    let _ = Events::new()?.insert(&event);

    println!("Time {}", &event_args.event_type);

    Ok(())
}
