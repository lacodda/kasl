//! Work-interval math for the daily report: the day sliced at its
//! pauses, short-interval filtering, and the end-time fallbacks.
//!
//! ```rust
//! use kasl::libs::report::{calculate_work_intervals, filter_short_intervals, WorkInterval};
//! use kasl::db::workdays::Workday;
//! use kasl::libs::pause::Pause;
//! use chrono::Local;
//!
//! let workday = Workday {
//!     id: 1,
//!     date: Local::now().date_naive(),
//!     start: Local::now().naive_local(),
//!     end: Some(Local::now().naive_local()),
//! };
//!
//! let pauses: Vec<Pause> = vec![/* pause data */];
//! let intervals = calculate_work_intervals(&workday, &pauses);
//!
//! // Filter short intervals for cleaner reporting
//! let (filtered_intervals, filter_info) = filter_short_intervals(&intervals, 30);
//! ```

use crate::libs::pause::Pause;
use crate::{db::workdays::Workday, libs::productivity::Productivity};
use anyhow::Result;
use chrono::{Duration, NaiveDateTime};

/// One uninterrupted stretch of work between pauses (or day bounds).
#[derive(Debug, Clone)]
pub struct WorkInterval {
    /// Workday start, or the end of the previous pause.
    pub start: NaiveDateTime,

    /// Start of the next pause, or the workday end.
    pub end: NaiveDateTime,

    /// `end - start`.
    pub duration: Duration,

    /// Index (into the original pause list) of the pause that ended this
    /// interval; `None` for the day's final interval.
    pub pause_after: Option<usize>,
}

impl WorkInterval {
    /// True when the interval is under `min_minutes`.
    ///
    /// ```rust
    /// use kasl::libs::report::WorkInterval;
    /// use chrono::{Duration, Local};
    ///
    /// let start_time = Local::now().naive_local();
    /// let interval = WorkInterval {
    ///     start: start_time,
    ///     end: start_time + Duration::minutes(20),
    ///     duration: Duration::minutes(20),
    ///     pause_after: Some(1),
    /// };
    ///
    /// assert_eq!(interval.is_short(30), true);  // 20 < 30
    /// assert_eq!(interval.is_short(15), false); // 20 >= 15
    /// ```
    pub fn is_short(&self, min_minutes: u64) -> bool {
        self.duration < Duration::minutes(min_minutes as i64)
    }
}

/// What fell under the short-interval threshold, and which pauses would
/// have to go to merge the fragments back together.
#[derive(Debug)]
pub struct ShortIntervalsInfo {
    pub count: usize,

    /// Combined length of all short intervals.
    pub total_duration: Duration,

    /// `(index in the original interval list, interval)` pairs.
    pub intervals: Vec<(usize, WorkInterval)>,

    /// Pause indices whose removal would merge each short interval into
    /// its predecessor; empty when only display filtering was requested.
    pub pauses_to_remove: Vec<usize>,
}

/// The moment the day effectively ends: the recorded end; else "now"
/// for today (guarded so a start ahead of the clock cannot make the day
/// negative); else, for an unclosed past day, the last observed pause
/// end - falling back to the start rather than stretching to "now".
pub fn workday_end_time(workday: &Workday, pauses: &[Pause]) -> chrono::NaiveDateTime {
    if let Some(end) = workday.end {
        return end;
    }

    let now = chrono::Local::now().naive_local();
    // Only while the day is still today, and only if the clock has actually
    // passed the start: a workday timestamped slightly ahead of the clock - a
    // DST shift, a corrected system time - would otherwise yield an end before
    // the start, and every duration computed from it would go negative.
    if workday.date == now.date() && now > workday.start {
        return now;
    }

    // A past day that was never closed: fall back to the last thing we observed.
    pauses
        .iter()
        .filter_map(|pause| pause.end)
        .max()
        .filter(|last| *last > workday.start)
        .unwrap_or(workday.start)
}

/// Slices the workday at its completed pauses into work intervals.
///
/// Pauses without an end are skipped; the rest are processed in
/// chronological order, and the tail interval runs to [`workday_end_time`].
///
/// ```rust
/// use kasl::libs::report::calculate_work_intervals;
/// use kasl::db::workdays::Workday;
/// use kasl::libs::pause::Pause;
/// use chrono::{Local, Duration};
///
/// let start_time = Local::now().naive_local();
/// let end_time = start_time + Duration::hours(8);
/// let lunch_start = start_time + Duration::hours(4);
/// let lunch_end = lunch_start + Duration::minutes(30);
/// let lunch_duration = Duration::minutes(30);
///
/// let workday = Workday {
///     id: 1,
///     date: start_time.date(),
///     start: start_time,
///     end: Some(end_time),
/// };
///
/// let pauses = vec![
///     Pause {
///         id: 1,
///         start: lunch_start,
///         end: Some(lunch_end),
///         duration: Some(lunch_duration),
///         protected: false,
///     },
/// ];
///
/// let intervals = calculate_work_intervals(&workday, &pauses);
/// println!("Generated {} work intervals", intervals.len());
/// ```
pub fn calculate_work_intervals(workday: &Workday, pauses: &[Pause]) -> Vec<WorkInterval> {
    // Determine workday end time (current time if still ongoing)
    let end_time = workday_end_time(workday, pauses);

    // Initialize interval collection and current time tracker
    let mut intervals = vec![];
    let mut current_time = workday.start;

    // Filter out incomplete pauses and sort chronologically
    // Only pauses with both start and end times can create work intervals
    let mut complete_pauses: Vec<(usize, &Pause)> = pauses.iter().enumerate().filter(|(_, pause)| pause.end.is_some()).collect();

    // Sort pauses by start time to ensure chronological processing
    complete_pauses.sort_by_key(|(_, pause)| pause.start);

    // Process each pause to create work intervals
    for (original_idx, pause) in complete_pauses {
        // Create work interval before this pause (if there's time)
        if current_time < pause.start {
            intervals.push(WorkInterval {
                start: current_time,
                end: pause.start,
                duration: pause.start - current_time,
                pause_after: Some(original_idx),
            });
        }

        // Move current time to the end of the pause
        if let Some(pause_end) = pause.end {
            current_time = pause_end;
        }
    }

    // Add the final work interval after the last pause (if there's time)
    if current_time < end_time {
        intervals.push(WorkInterval {
            start: current_time,
            end: end_time,
            duration: end_time - current_time,
            pause_after: None, // No pause after the final interval
        });
    }

    intervals
}

/// Finds intervals under the threshold and the pauses whose removal
/// would merge them away; `None` when there are none.
///
/// ```rust
/// use kasl::libs::report::{analyze_short_intervals, WorkInterval};
///
/// let intervals = vec![/* work intervals */];
/// let min_duration = 30; // 30-minute minimum
///
/// match analyze_short_intervals(&intervals, min_duration) {
///     Some(analysis) => {
///         println!("Found {} short intervals", analysis.count);
///         println!("Total fragmented time: {:?}", analysis.total_duration);
///         println!("Optimization: remove pauses {:?}", analysis.pauses_to_remove);
///     },
///     None => {
///         println!("No short intervals detected - work patterns are optimal");
///     }
/// }
/// ```
///
pub fn analyze_short_intervals(intervals: &[WorkInterval], min_minutes: u64) -> Option<ShortIntervalsInfo> {
    // Collect all intervals that fall below the minimum duration threshold
    let mut short_intervals = Vec::new();
    let mut total_duration = Duration::zero();
    let mut pauses_to_remove = Vec::new();

    // Analyze each interval for duration and optimization opportunities
    for (idx, interval) in intervals.iter().enumerate() {
        if interval.is_short(min_minutes) {
            // Record this short interval for analysis
            short_intervals.push((idx, interval.clone()));
            total_duration += interval.duration;

            // Identify optimization opportunity: remove the pause that created this interval
            // To remove a short interval, we need to remove the pause before it
            // (which connects it to the previous interval)
            if idx > 0 {
                // Get the pause that created this interval by ending the previous one
                if let Some(pause_idx) = intervals[idx - 1].pause_after {
                    pauses_to_remove.push(pause_idx);
                }
            }
        }
    }

    // Return analysis results only if short intervals were found
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

/// Splits intervals into (kept, dropped-as-short) at display time - the
/// database is never modified by this filter.
///
/// ```rust
/// use kasl::libs::report::{calculate_work_intervals, filter_short_intervals};
/// use kasl::db::workdays::Workday;
/// use kasl::libs::pause::Pause;
/// use chrono::Local;
///
/// let workday = Workday {
///     id: 1,
///     date: Local::now().date_naive(),
///     start: Local::now().naive_local(),
///     end: Some(Local::now().naive_local()),
/// };
/// let pauses: Vec<Pause> = vec![];
///
/// let intervals = calculate_work_intervals(&workday, &pauses);
/// let (filtered, info) = filter_short_intervals(&intervals, 30);
///
/// if let Some(info) = info {
///     println!("Filtered {} short intervals", info.count);
/// }
/// ```
pub fn filter_short_intervals(intervals: &[WorkInterval], min_minutes: u64) -> (Vec<WorkInterval>, Option<ShortIntervalsInfo>) {
    let mut filtered_intervals = Vec::new();
    let mut short_intervals = Vec::new();
    let mut total_duration = Duration::zero();

    for (idx, interval) in intervals.iter().enumerate() {
        if interval.is_short(min_minutes) {
            // This is a short interval - add to filtered list
            short_intervals.push((idx, interval.clone()));
            total_duration += interval.duration;
        } else {
            // This interval meets minimum duration - keep it
            filtered_intervals.push(interval.clone());
        }
    }

    let filtered_info = if short_intervals.is_empty() {
        None
    } else {
        Some(ShortIntervalsInfo {
            count: short_intervals.len(),
            total_duration,
            intervals: short_intervals,
            pauses_to_remove: Vec::new(), // Not needed for display filtering
        })
    };

    (filtered_intervals, filtered_info)
}

/// Returns `(displayed duration, productivity)` for the report: the
/// duration sums the (already filtered) intervals, while productivity
/// comes from the central [`Productivity`] calculation so every command
/// shows the same figure.
///
/// ```rust,no_run
/// # fn f() -> anyhow::Result<()> {
/// use kasl::libs::report::{report_with_intervals, WorkInterval};
/// use kasl::libs::formatter::format_duration;
/// use kasl::db::workdays::Workday;
/// use chrono::Local;
///
/// let workday = Workday {
///     id: 1,
///     date: Local::now().date_naive(),
///     start: Local::now().naive_local(),
///     end: Some(Local::now().naive_local()),
/// };
/// let filtered_intervals: Vec<WorkInterval> = vec![];
///
/// let (duration, productivity) = report_with_intervals(&workday, &filtered_intervals)?;
/// println!("Work time: {}, Productivity: {:.1}%", format_duration(&duration), productivity);
/// # Ok(())
/// # }
/// ```
pub fn report_with_intervals(workday: &Workday, intervals: &[WorkInterval]) -> Result<(Duration, f64)> {
    // Calculate filtered duration based on provided intervals (for display purposes)
    let filtered_duration = intervals.iter().fold(Duration::zero(), |acc, interval| acc + interval.duration);

    // Use centralized productivity module for consistent, comprehensive calculation
    let productivity = Productivity::new(workday)?.calculate_productivity();

    Ok((filtered_duration, productivity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate, NaiveDateTime};

    fn at(date: NaiveDate, h: u32, m: u32) -> NaiveDateTime {
        date.and_hms_opt(h, m, 0).unwrap()
    }

    fn workday(date: NaiveDate, start_h: u32, end: Option<NaiveDateTime>) -> Workday {
        Workday {
            id: 1,
            date,
            start: at(date, start_h, 0),
            end,
        }
    }

    fn pause(date: NaiveDate, from: (u32, u32), to: (u32, u32)) -> Pause {
        let start = at(date, from.0, from.1);
        let end = at(date, to.0, to.1);
        Pause::detected(1, start, Some(end), Some(end - start))
    }

    #[test]
    fn recorded_end_is_used_as_is() {
        let date = NaiveDate::from_ymd_opt(2025, 8, 22).unwrap();
        let end = at(date, 18, 0);
        let wd = workday(date, 9, Some(end));
        assert_eq!(workday_end_time(&wd, &[]), end);
    }

    #[test]
    fn unclosed_past_day_ends_at_last_pause_not_now() {
        // Regression: "now" as the fallback stretched an unclosed August day
        // across every hour since, reporting thousands of hours.
        let date = NaiveDate::from_ymd_opt(2025, 8, 22).unwrap();
        let wd = workday(date, 9, None);
        let pauses = [pause(date, (12, 0), (12, 30)), pause(date, (16, 0), (16, 43))];

        let end = workday_end_time(&wd, &pauses);

        assert_eq!(end, at(date, 16, 43));
        assert!(end - wd.start < Duration::hours(24));
    }

    #[test]
    fn unclosed_past_day_without_pauses_collapses_to_start() {
        let date = NaiveDate::from_ymd_opt(2025, 8, 22).unwrap();
        let wd = workday(date, 9, None);
        assert_eq!(workday_end_time(&wd, &[]), wd.start);
    }

    #[test]
    fn unclosed_today_never_ends_before_it_starts() {
        // A start slightly ahead of the clock - DST, a corrected system time -
        // must not produce a negative-length day, which read as 0% productivity.
        let now = chrono::Local::now().naive_local();
        let wd = workday(now.date(), 0, None);
        let wd = Workday {
            start: now + Duration::hours(2),
            ..wd
        };

        let end = workday_end_time(&wd, &[]);

        assert!(end >= wd.start, "end {end} precedes start {}", wd.start);
    }

    #[test]
    fn unclosed_today_still_runs_to_now() {
        let today = chrono::Local::now().naive_local();
        let wd = workday(today.date(), 0, None);

        let end = workday_end_time(&wd, &[]);

        // Ongoing day: end tracks the current moment rather than a past pause.
        assert!((end - today).num_seconds().abs() < 5);
    }
}
