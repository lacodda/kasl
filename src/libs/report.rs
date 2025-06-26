//! Contains shared logic for report generation.

use crate::db::workdays::Workday;
use crate::libs::pause::Pause;
use chrono::{Duration, NaiveDateTime};

/// Represents a single continuous work interval.
pub struct WorkInterval {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub duration: Duration,
}

/// Calculates work intervals for a given day based on pauses.
///
/// It takes the start and end of the workday and subtracts the time for pauses,
/// returning a vector of continuous work intervals.
pub fn calculate_work_intervals(workday: &Workday, pauses: &[Pause]) -> Vec<WorkInterval> {
    let end_time = workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());
    let mut intervals = vec![];
    let mut current_time = workday.start;

    // Filter out pauses without an end time and sort them chronologically.
    let mut pauses_iter = pauses.iter().filter(|p| p.end.is_some()).collect::<Vec<_>>();
    pauses_iter.sort_by_key(|p| p.start);

    for pause in pauses_iter {
        // Add the work interval before the pause.
        if current_time < pause.start {
            intervals.push(WorkInterval {
                start: current_time,
                end: pause.start,
                duration: pause.start - current_time,
            });
        }
        // Move the current time to the end of the pause.
        if let Some(pause_end) = pause.end {
            current_time = pause_end;
        }
    }

    // Add the final work interval after the last pause.
    if current_time < end_time {
        intervals.push(WorkInterval {
            start: current_time,
            end: end_time,
            duration: end_time - current_time,
        });
    }

    intervals
}
