//! Work interval calculation and productivity analysis for daily reports.
//!
//! Provides core logic for analyzing work patterns and generating detailed reports
//! about productivity, work intervals, and break patterns.
//!
//! ## Usage
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

/// Represents a single continuous work interval between breaks.
///
/// This structure captures a period of uninterrupted work time, providing
/// the foundation for productivity analysis and reporting. Each interval
/// represents a focused work session bounded by either the workday start/end
/// or pause periods.
///
/// ## Interval Boundaries
///
/// Work intervals are defined by:
/// - **Start Time**: When focused work began (workday start or end of previous pause)
/// - **End Time**: When focused work ended (start of next pause or workday end)
/// - **Duration**: Total time spent in focused work during this period
///
/// ## Pause Association
///
/// Each interval can be associated with the pause that follows it:
/// - **Some(index)**: Index of the pause that ended this work interval
/// - **None**: This interval extends to the end of the workday
///
/// This association enables:
/// - Analysis of work-break patterns
/// - Identification of interruption causes
/// - Optimization recommendations for pause timing
///
/// ## Usage Context
///
/// Work intervals are used for:
/// - Productivity calculation and analysis
/// - Generating detailed work reports
/// - Identifying optimization opportunities
/// - Visualizing work patterns in charts and graphs
#[derive(Debug, Clone)]
pub struct WorkInterval {
    /// The timestamp when this work interval began.
    ///
    /// This is either the workday start time (for the first interval)
    /// or the end time of the previous pause (for subsequent intervals).
    /// Represents the moment when focused work activity resumed.
    pub start: NaiveDateTime,

    /// The timestamp when this work interval ended.
    ///
    /// This is either the start time of the next pause (for most intervals)
    /// or the workday end time (for the final interval). Represents the
    /// moment when focused work was interrupted or completed.
    pub end: NaiveDateTime,

    /// The total duration of focused work during this interval.
    ///
    /// Calculated as `end - start`, this represents the net productive
    /// time during this period. Used for productivity calculations,
    /// efficiency analysis, and time accounting in reports.
    pub duration: Duration,

    /// Optional reference to the pause that follows this interval.
    ///
    /// Contains the index of the pause in the original pause collection
    /// that ended this work interval. `None` indicates this interval
    /// extends to the end of the workday without interruption.
    ///
    /// ## Usage Notes
    /// - Used for analyzing work-break patterns
    /// - Enables identification of frequent interruption points
    /// - Supports optimization recommendations for pause timing
    /// - Links intervals to specific causes of work interruption
    pub pause_after: Option<usize>, // Index of pause after this interval
}

impl WorkInterval {
    /// Determines if this interval is shorter than the specified minimum duration.
    ///
    /// This method is used to identify "short intervals" that may indicate
    /// excessive interruptions or poor work habits. Short intervals often
    /// represent brief periods of work between frequent breaks, which can
    /// significantly impact overall productivity.
    ///
    /// ## Usage in Analysis
    ///
    /// Short intervals are identified for:
    /// - **Productivity Analysis**: Understanding interruption patterns
    /// - **Optimization Recommendations**: Suggesting pause consolidation
    /// - **Work Habit Assessment**: Identifying areas for improvement
    /// - **Focus Period Analysis**: Measuring sustained work capability
    ///
    /// ## Threshold Considerations
    ///
    /// Common minimum duration thresholds:
    /// - **15 minutes**: Very strict, identifies micro-interruptions
    /// - **30 minutes**: Moderate, focuses on meaningful work blocks
    /// - **60 minutes**: Lenient, identifies only major fragmentation
    ///
    /// # Examples
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

/// Information about short intervals detected in a workday.
///
/// This structure provides comprehensive analysis of work intervals that
/// fall below the minimum duration threshold. It includes both statistical
/// information about the impact of short intervals and actionable
/// recommendations for optimization.
///
/// ## Analysis Components
///
/// ### Statistical Information
/// - **Count**: Number of short intervals detected
/// - **Total Duration**: Cumulative time spent in short work periods
/// - **Individual Intervals**: Specific intervals with their details
///
/// ### Optimization Recommendations
/// - **Pauses to Remove**: Specific pauses that could be eliminated
/// - **Merge Opportunities**: Intervals that could be combined
/// - **Productivity Impact**: Potential time savings from optimization
///
/// ## Usage Context
///
/// This analysis is used for:
/// - Generating optimization recommendations in reports
/// - Identifying patterns of work fragmentation
/// - Calculating potential productivity improvements
/// - Providing actionable feedback to users
#[derive(Debug)]
pub struct ShortIntervalsInfo {
    /// The number of intervals that fall below the minimum duration threshold.
    ///
    /// This count provides a quick assessment of work fragmentation:
    /// - **0**: No fragmentation issues detected
    /// - **1-3**: Minor fragmentation, limited impact
    /// - **4+**: Significant fragmentation, optimization recommended
    pub count: usize,

    /// The cumulative duration of all short intervals combined.
    ///
    /// Represents the total amount of time spent in fragmented work
    /// periods. This metric helps quantify the impact of work
    /// interruptions and provides context for optimization efforts.
    ///
    /// ## Impact Assessment
    /// - **< 30 minutes**: Minor impact on overall productivity
    /// - **30-60 minutes**: Moderate impact, optimization beneficial
    /// - **> 60 minutes**: Significant impact, optimization essential
    pub total_duration: Duration,

    /// Detailed information about each short interval detected.
    ///
    /// Each tuple contains:
    /// - **Index**: Position of the interval in the original collection
    /// - **WorkInterval**: Complete interval data with timing information
    ///
    /// This detailed information enables:
    /// - Specific analysis of each fragmented period
    /// - Identification of patterns in interruption timing
    /// - Targeted recommendations for specific intervals
    pub intervals: Vec<(usize, WorkInterval)>, // (index, interval)

    /// Indices of pauses that could be removed to merge short intervals.
    ///
    /// These pause indices represent optimization opportunities where
    /// removing or consolidating breaks could create longer, more
    /// productive work intervals. The indices correspond to positions
    /// in the original pause collection.
    ///
    /// ## Optimization Strategy
    /// - **Pause Removal**: Eliminate unnecessary short breaks
    /// - **Pause Consolidation**: Combine multiple short breaks into fewer, longer ones
    /// - **Timing Adjustment**: Shift break timing to create better work blocks
    ///
    /// ## Implementation Notes
    /// To remove a short interval, remove the pause that created it by
    /// separating it from the previous interval. This effectively merges
    /// the short interval with its predecessor.
    pub pauses_to_remove: Vec<usize>, // Indices of pauses that create short intervals
}

/// Calculates work intervals for a given workday based on pause records.
///
/// This function performs the core algorithm for converting raw workday and
/// pause data into a structured collection of work intervals. It handles the
/// complexity of time calculations, pause filtering, and interval boundary
/// determination to produce accurate work period analysis.
///
/// ## Algorithm Overview
///
/// 1. **Initialization**: Start with workday boundaries and empty interval list
/// 2. **Pause Filtering**: Remove incomplete pauses and sort chronologically
/// 3. **Interval Generation**: Create work periods between consecutive pauses
/// 4. **Boundary Handling**: Handle workday start/end as interval boundaries
/// 5. **Duration Calculation**: Compute accurate durations for each interval
///
/// ## Pause Processing
///
/// The function handles various pause scenarios:
/// - **Complete Pauses**: Have both start and end times
/// - **Incomplete Pauses**: Missing end times (filtered out)
/// - **Overlapping Pauses**: Handled through chronological sorting
/// - **Out-of-bounds Pauses**: Pauses outside workday boundaries
///
/// ## Edge Cases Handled
///
/// - **No Pauses**: Single interval covering entire workday
/// - **Workday Boundaries**: Pauses at start/end of workday
/// - **Consecutive Pauses**: Multiple pauses with no work time between
/// - **Invalid Times**: Pauses with end time before start time
///
/// # Examples
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
///     // ... other fields
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
///     // ... more pauses
/// ];
///
/// let intervals = calculate_work_intervals(&workday, &pauses);
/// println!("Generated {} work intervals", intervals.len());
/// ```
///
/// # Performance Considerations
///
/// - **Time Complexity**: O(n log n) due to pause sorting
/// - **Space Complexity**: O(n) for interval storage
/// - **Memory Usage**: Minimal allocation during processing
///
/// Resolves the effective end of a workday that has no recorded end time.
///
/// A workday stays open when the daemon was killed or the machine slept before
/// `kasl end` ran. "Now" is only a sensible stand-in while the day is still
/// today; for any earlier date it would stretch the day across every hour since,
/// which is how an unclosed August day reported 8425 hours. For a past date the
/// day is closed at the last evidence of activity - the end of its final pause,
/// or its start when nothing else is known.
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

/// Analyzes work intervals to identify short periods that may indicate poor productivity.
///
/// This function performs comprehensive analysis of work intervals to identify
/// periods that fall below the minimum duration threshold. It provides both
/// statistical analysis and actionable optimization recommendations to help
/// users improve their work patterns and productivity.
///
/// ## Analysis Process
///
/// 1. **Threshold Filtering**: Identify intervals shorter than minimum duration
/// 2. **Statistical Calculation**: Compute total count and cumulative duration
/// 3. **Optimization Analysis**: Identify pauses that could be removed
/// 4. **Recommendation Generation**: Provide specific improvement suggestions
///
/// ## Optimization Logic
///
/// Short intervals are typically created by pauses that interrupt focused work:
/// - **Pause Identification**: Find pauses that create short intervals
/// - **Merge Opportunities**: Identify intervals that could be combined
/// - **Impact Assessment**: Calculate potential productivity improvements
///
/// ## Return Value Analysis
///
/// - **Some(info)**: Short intervals detected, optimization possible
/// - **None**: No short intervals found, work patterns are optimal
///
/// # Examples
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
/// # Optimization Recommendations
///
/// The function provides specific recommendations:
/// - **Pause Removal**: Eliminate unnecessary short breaks
/// - **Break Consolidation**: Combine multiple short breaks
/// - **Timing Adjustment**: Reschedule breaks to preserve focus periods
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

/// Filters out short work intervals from the provided interval list.
///
/// This function removes work intervals that are shorter than the specified
/// minimum duration, providing cleaner reporting by eliminating brief
/// interruptions that don't represent meaningful work periods. This is the
/// new approach for handling short intervals - filtering at display time
/// instead of modifying the database.
///
/// ## Filtering Logic
///
/// - Intervals shorter than `min_minutes` are excluded from the result
/// - Remaining intervals maintain their original timing and properties
/// - No database changes are made - this is purely a display/API filter
/// - Used by both `kasl report` (display) and `kasl report --send` (API submission)
///
/// ## Return Value
///
/// Returns a tuple containing:
/// - **Filtered intervals**: Only intervals meeting the minimum duration
/// - **Filtered intervals info**: Analysis of what was filtered out (if any)
///
/// # Examples
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

/// Process daily work report data using pre-calculated intervals.
///
/// This function handles the data processing for daily work reports, calculating
/// productivity metrics and work durations. It leverages the centralized `Productivity`
/// module for consistent calculations across the application.
///
/// ## Calculation Method
///
/// The function uses two different approaches for different metrics:
/// - **Filtered Duration**: Summed directly from provided intervals (for display purposes)
/// - **Productivity**: Calculated using the comprehensive `Productivity::calculate_productivity()`
///   method which properly handles all pause types, breaks, and overlaps
///
/// This separation allows for interval-based filtering (for clean reports) while maintaining
/// accurate productivity calculations that account for all time categories.
///
/// ## Data Consistency
///
/// By using `Productivity::new()`, this function automatically:
/// - Loads the same data used throughout the application
/// - Applies consistent calculation logic
/// - Handles all edge cases and data integrity issues
///
/// # Examples
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
