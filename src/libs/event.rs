use chrono::{
    prelude::{Local, NaiveDateTime},
    Datelike, Duration, NaiveDate,
};
use clap::ValueEnum;
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

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

pub trait EventGroup {
    fn merge(self) -> Vec<Event>;
    fn group_events(self) -> HashMap<NaiveDate, Vec<Event>>;
    fn update_duration(&self) -> Vec<Event>;
    fn total_duration(&mut self) -> (Vec<Event>, Duration);
    fn format(&mut self) -> Vec<FormatEvent>;
}

impl EventGroup for Vec<Event> {
    fn merge(self) -> Vec<Event> {
        let mut merged = vec![];
        let mut iter = self.into_iter();

        if let Some(mut current) = iter.next() {
            for next in iter {
                if current.end.is_none() {
                    current.end = Some(next.start);
                }
                let duration = next.start.signed_duration_since(current.end.unwrap()).num_seconds().abs();
                if duration < DURATION {
                    current.end = next.end;
                } else {
                    merged.push(current);
                    current = next;
                }
            }
            if current.end.is_none() {
                current.end = Some(Local::now().naive_local());
            }
            merged.push(current);
        }
        merged
    }

    fn group_events(self) -> HashMap<NaiveDate, Vec<Event>> {
        let mut events: HashMap<NaiveDate, Vec<Event>> = HashMap::new();
        for event in self.into_iter() {
            let event_date = event.start.date();
            events.entry(event_date).or_insert_with(Vec::new).push(event);
        }

        events
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

    fn format(&mut self) -> Vec<FormatEvent> {
        let mut events = vec![];
        for (index, event) in self.iter().enumerate() {
            let mut end = "-".to_string();
            let duration = "".to_string();
            if event.end.is_some() {
                end = event.end.unwrap().format("%H:%M").to_string();
            }
            events.push(FormatEvent {
                id: (index + 1) as i32,
                start: event.start.format("%H:%M").to_string(),
                end,
                duration,
            })
        }

        events
    }
}

pub trait EventGroupDuration {
    fn calc(self) -> (HashMap<NaiveDate, (Vec<Event>, Duration)>, Duration);
}

impl EventGroupDuration for HashMap<NaiveDate, Vec<Event>> {
    fn calc(self) -> (HashMap<NaiveDate, (Vec<Event>, Duration)>, Duration) {
        let mut event_group: HashMap<NaiveDate, (Vec<Event>, Duration)> = HashMap::new();
        for (date, events) in self.iter() {
            let day_events = events.clone().merge().update_duration().total_duration();
            event_group.insert(*date, day_events);
        }
        (event_group, Duration::zero())
    }
}

pub trait EventGroupTotalDuration {
    fn add_rest_dates(&mut self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> (HashMap<NaiveDate, (Vec<Event>, Duration)>, Duration);
    fn total_duration(&mut self) -> (HashMap<NaiveDate, (Vec<Event>, Duration)>, Duration);
    fn format(&mut self) -> (HashMap<NaiveDate, (Vec<FormatEvent>, String)>, String, String);
}

impl EventGroupTotalDuration for (HashMap<NaiveDate, (Vec<Event>, Duration)>, Duration) {
    fn add_rest_dates(&mut self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> (HashMap<NaiveDate, (Vec<Event>, Duration)>, Duration) {
        let mut current_month_rest_dates: HashMap<NaiveDate, (Vec<Event>, Duration)> = rest_dates
            .iter()
            .filter(|&&date| date.month() == Local::now().naive_local().month())
            .map(|&date| (date, (vec![], duration)))
            .collect();

        for (date, events) in self.0.iter() {
            let mut event_group_duration = events.clone();
            if rest_dates.contains(date) {
                event_group_duration.1 += duration;
            }
            current_month_rest_dates.insert(*date, event_group_duration);
        }

        (current_month_rest_dates, self.1)
    }

    fn total_duration(&mut self) -> (HashMap<NaiveDate, (Vec<Event>, Duration)>, Duration) {
        let mut total_duration = self.1;
        for (_, (_, duration)) in self.0.iter() {
            total_duration += *duration;
        }
        (self.0.clone(), total_duration)
    }

    fn format(&mut self) -> (HashMap<NaiveDate, (Vec<FormatEvent>, String)>, String, String) {
        let mut event_group: HashMap<NaiveDate, (Vec<FormatEvent>, String)> = HashMap::new();
        for (date, events) in self.0.iter() {
            event_group.insert(*date, events.clone().format());
        }

        let count = self.0.len() as i64;
        let mut average = Duration::seconds(0);
        if count > 0 {
            let average_sec = self.1.num_seconds() / count;
            average = Duration::seconds(average_sec);
        }

        (event_group, FormatEvent::format_duration(Some(self.1)), FormatEvent::format_duration(Some(average)))
    }
}

#[derive(Debug, Clone)]
pub struct FormatEvent {
    pub id: i32,
    pub start: String,
    pub end: String,
    pub duration: String,
}

impl FormatEvent {
    pub fn format_duration(duration_opt: Option<Duration>) -> String {
        duration_opt.map_or_else(
            || "--:--".to_string(),
            |duration| {
                let hours = duration.num_hours();
                let mins = duration.num_minutes() % 60;
                format!("{:02}:{:02}", hours, mins)
            },
        )
    }
}

pub trait FormatEvents {
    fn format(&mut self) -> (Vec<FormatEvent>, String);
}

impl FormatEvents for (Vec<Event>, Duration) {
    fn format(&mut self) -> (Vec<FormatEvent>, String) {
        let mut events = vec![];
        for (index, event) in self.0.iter().enumerate() {
            events.push(FormatEvent {
                id: (index + 1) as i32,
                start: event.start.format("%H:%M").to_string(),
                end: event.end.unwrap().format("%H:%M").to_string(),
                duration: FormatEvent::format_duration(event.duration),
            })
        }

        (events, FormatEvent::format_duration(Some(self.1)))
    }
}
