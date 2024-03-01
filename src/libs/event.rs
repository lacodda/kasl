use chrono::{
    prelude::{Local, NaiveDateTime},
    Duration,
};
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
    pub duration: Option<Duration>,
}

impl Event {
    fn with_calculated_duration(&self) -> Self {
        match self.end {
            Some(end) => Self {
                duration: Some(end.signed_duration_since(self.start)),
                ..*self
            },
            None => Self { ..*self },
        }
    }
}

pub trait MergeEvents {
    fn merge(self) -> Vec<Event>;
    fn update_duration(&self) -> Vec<Event>;
    fn total_duration(&mut self) -> (Vec<Event>, Duration);
}

impl MergeEvents for Vec<Event> {
    fn merge(self) -> Vec<Event> {
        let mut merged = vec![];
        let mut iter = self.into_iter();

        if let Some(mut current) = iter.next() {
            for next in iter {
                let now = Local::now();
                let duration = next.start.signed_duration_since(current.end.unwrap_or(now.naive_local())).num_seconds().abs();

                if duration < DURATION {
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

    fn update_duration(&self) -> Vec<Event> {
        self.iter().map(|event| event.with_calculated_duration()).collect()
    }

    fn total_duration(&mut self) -> (Vec<Event>, Duration) {
        let mut total_duration = Duration::zero();
        for event in self.iter() {
            if let Some(duration) = event.duration {
                total_duration = total_duration + duration;
            }
        }
        (self.clone(), total_duration)
    }
}
