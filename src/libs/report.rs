//! Contains shared logic for report generation.

use crate::db::workdays::Workday;
use crate::libs::pause::Pause;
use chrono::{Duration, NaiveDateTime};

/// Represents a single continuous work interval.
#[derive(Debug, Clone)]
pub struct WorkInterval {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub duration: Duration,
    pub pause_after: Option<usize>, // Index of pause after this interval
}

impl WorkInterval {
    /// Check if this interval is shorter than the minimum duration
    pub fn is_short(&self, min_minutes: u64) -> bool {
        self.duration < Duration::minutes(min_minutes as i64)
    }
}

/// Information about short intervals found
#[derive(Debug)]
pub struct ShortIntervalsInfo {
    pub count: usize,
    pub total_duration: Duration,
    pub intervals: Vec<(usize, WorkInterval)>, // (index, interval)
    pub pauses_to_remove: Vec<usize>,          // Indices of pauses that create short intervals
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
    let mut pauses_with_indices: Vec<(usize, &Pause)> = pauses.iter().enumerate().filter(|(_, p)| p.end.is_some()).collect();
    pauses_with_indices.sort_by_key(|(_, p)| p.start);

    for (_pause_idx, (original_idx, pause)) in pauses_with_indices.iter().enumerate() {
        // Add the work interval before the pause.
        if current_time < pause.start {
            intervals.push(WorkInterval {
                start: current_time,
                end: pause.start,
                duration: pause.start - current_time,
                pause_after: Some(*original_idx),
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
            pause_after: None,
        });
    }

    intervals
}

/// Analyze work intervals to find short ones
pub fn analyze_short_intervals(intervals: &[WorkInterval], min_minutes: u64) -> Option<ShortIntervalsInfo> {
    let mut short_intervals = Vec::new();
    let mut total_duration = Duration::zero();
    let mut pauses_to_remove = Vec::new();

    for (idx, interval) in intervals.iter().enumerate() {
        if interval.is_short(min_minutes) {
            short_intervals.push((idx, interval.clone()));
            total_duration = total_duration + interval.duration;

            // To remove a short interval, we need to remove the pause before it
            // (which connects it to the previous interval)
            if idx > 0 {
                // Get the pause that created this interval
                if let Some(pause_idx) = intervals[idx - 1].pause_after {
                    pauses_to_remove.push(pause_idx);
                }
            }
        }
    }

    if short_intervals.is_empty() {
        None
    } else {
        Some(ShortIntervalsInfo {
            count: short_intervals.len(),
            total_duration,
            intervals: short_intervals,
            pauses_to_remove,
        })
    }
}
