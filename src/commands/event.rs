use clap::{Args, ValueEnum};
use std::{error::Error, fmt};
use crate::libs::db::Db;

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum EventType {
    #[default]
    Start,
    End,
    StartBreak,
    EndBreak,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct Event {
    pub id: Option<i32>,
    pub timestamp: Option<String>,
    pub event_type: EventType,
}

impl Event {
    fn new(event_type: &EventType) -> Self {
        Event {
            id: None,
            timestamp: None,
            event_type: *event_type,
        }
    }
}

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
    let _ = Db::new()?.insert_event(&event);
    
    println!("Time {}", &event_args.event_type);

    Ok(())
}
