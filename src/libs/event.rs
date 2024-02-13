use clap::ValueEnum;
use std::fmt;

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
    pub fn new(event_type: &EventType) -> Self {
        Event {
            id: None,
            timestamp: None,
            event_type: *event_type,
        }
    }
}
