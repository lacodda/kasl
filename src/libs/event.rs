use clap::ValueEnum;
use std::fmt;

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum EventType {
    #[default]
    Start,
    End,
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct Event {
    pub id: Option<i32>,
    pub start: Option<String>,
    pub end: Option<String>,
}

impl Event {
    pub fn new() -> Self {
        Event { id: None, start: None, end: None }
    }
}
