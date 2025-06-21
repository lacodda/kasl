use crate::libs::event::FormatEvent;
use chrono::{prelude::NaiveDateTime, Duration};

#[derive(Debug, Clone)]
pub struct Break {
    pub id: i32,
    pub start: NaiveDateTime,       // TIMESTAMP as YYYY-MM-DD HH:MM:SS
    pub end: Option<NaiveDateTime>, // TIMESTAMP as YYYY-MM-DD HH:MM:SS
    pub duration: Option<Duration>, // Duration in seconds
}

pub trait BreakGroup {
    fn format(&mut self) -> Vec<FormatEvent>;
}

impl BreakGroup for Vec<Break> {
    fn format(&mut self) -> Vec<FormatEvent> {
        let mut breaks = vec![];
        for (index, b) in self.iter().enumerate() {
            breaks.push(FormatEvent {
                id: (index + 1) as i32,
                start: b.start.format("%H:%M").to_string(),
                end: b.end.unwrap().format("%H:%M").to_string(),
                duration: FormatEvent::format_duration(b.duration),
            })
        }

        breaks
    }
}
