use clap::{Args, ValueEnum};
use std::{error::Error, fmt};

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

// impl

#[derive(Debug)]
pub struct Event {
    pub id: Option<i32>,
    pub timestamp: Option<String>,
    pub event_type: EventType,
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
    println!("Time {}", &event_args.event_type);

    Ok(())
}
