use chrono::prelude::*;
use clap::ValueEnum;
use std::fmt;

const DURATION: i64 = 20 * 60; // 20 mins

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

#[derive(Debug, Clone)]
pub struct Event {
    pub id: i32,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
}

pub trait MergeEvents {
    fn merge(self) -> Vec<Event>;
}

impl MergeEvents for Vec<Event> {
    fn merge(self) -> Vec<Event> {
        let mut merged = vec![];
        let mut iter = self.into_iter();

        if let Some(mut current) = iter.next() {
            for next in iter {
                let now = Utc::now();
                let duration = next.start.signed_duration_since(current.end.unwrap_or(now.naive_utc())).num_seconds().abs();
                if duration <= DURATION {
                    current.end = next.end;
                } else {
                    merged.push(current);
                    current = next;
                }
            }
            merged.push(current);
        }
        merged
    }
}
