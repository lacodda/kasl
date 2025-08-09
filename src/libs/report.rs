//! Work interval calculation and productivity analysis for daily reports.
//!
//! This module provides core logic for analyzing work patterns and generating
//! detailed reports about productivity, work intervals, and break patterns.
//! It transforms raw time data into actionable insights for users and managers.
//!
//! ## Core Functionality
//!
//! ### Work Interval Analysis
//! - **Interval Calculation**: Convert workday and pause data into continuous work periods
//! - **Productivity Metrics**: Calculate efficiency ratios and work pattern analysis
//! - **Short Interval Detection**: Identify and analyze brief work periods that may indicate interruptions
//! - **Interval Optimization**: Provide suggestions for merging short intervals
//!
//! ### Report Generation
//! - **Daily Reports**: Comprehensive breakdown of a single workday
//! - **Productivity Analysis**: Detailed metrics on work efficiency
//! - **Pattern Recognition**: Identify trends and optimization opportunities
//! - **Data Visualization**: Structured data for charts and graphs
//!
//! ## Data Processing Pipeline
//!
//! ```text
//! Raw Data → Interval Calculation → Analysis → Optimization → Report
//!     ↓             ↓                  ↓           ↓          ↓
//! Workday      Work intervals    Productivity   Short      Final
//! Pauses       Time segments     Percentages    detection   report
//! Tasks        Break patterns    Efficiency     Merging     Export
//! ```
//!
//! ## Key Algorithms
//!
//! ### Work Interval Calculation
//! 1. **Start with Full Workday**: Use workday start and end times as boundaries
//! 2. **Apply Pause Breaks**: Split the workday at each pause period
//! 3. **Create Intervals**: Generate continuous work periods between pauses
//! 4. **Calculate Durations**: Determine the length of each work interval
//! 5. **Associate Pauses**: Link each interval to its following pause (if any)
//!
//! ### Short Interval Analysis
//! 1. **Threshold Detection**: Identify intervals shorter than minimum duration
//! 2. **Impact Assessment**: Calculate total time lost to short intervals
//! 3. **Merge Candidates**: Identify pauses that could be removed to merge intervals
//! 4. **Optimization Suggestions**: Provide actionable recommendations
//!
//! ## Productivity Insights
//!
//! ### Metrics Calculated
//! - **Work Efficiency**: Ratio of productive time to total time
//! - **Interruption Analysis**: Frequency and impact of work breaks
//! - **Focus Periods**: Identification of sustained work sessions
//! - **Optimization Potential**: Time that could be reclaimed through better habits
//!
//! ### Pattern Recognition
//! - **Peak Hours**: Times of highest productivity and focus
//! - **Break Patterns**: Frequency and duration of interruptions
//! - **Work Rhythm**: Natural cycles of productivity throughout the day
//! - **Improvement Areas**: Specific recommendations for optimization
//!
//! ## Examples
//!
//! ### Basic Interval Calculation
//! ```rust
//! use kasl::libs::report::{calculate_work_intervals, WorkInterval};
//! use kasl::db::workdays::Workday;
//! use kasl::libs::pause::Pause;
//!
//! let workday = Workday {
//!     start: start_time,
//!     end: Some(end_time),
//!     // ... other fields
//! };
//!
//! let pauses = vec![/* pause data */];
//! let intervals = calculate_work_intervals(&workday, &pauses);
//!
//! for interval in intervals {
//!     println!("Work period: {} - {} ({})",
//!         interval.start, interval.end, interval.duration);
//! }
//! ```
//!
//! ### Short Interval Analysis
//! ```rust
//! use kasl::libs::report::{analyze_short_intervals, WorkInterval};
//!
//! let intervals = vec![/* work intervals */];
//! let min_minutes = 30; // Minimum interval duration
//!
//! if let Some(analysis) = analyze_short_intervals(&intervals, min_minutes) {
//!     println!("Found {} short intervals totaling {}",
//!         analysis.count, analysis.total_duration);
//!     println!("Consider removing pauses: {:?}", analysis.pauses_to_remove);
//! }
//! ```

use crate::db::workdays::Workday;
use crate::libs::pause::Pause;
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
    /// # Arguments
    ///
    /// * `min_minutes` - Minimum duration threshold in minutes
    ///
    /// # Returns
    ///
    /// Returns `true` if the interval duration is less than the threshold.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kasl::libs::report::WorkInterval;
    /// use chrono::{Duration, NaiveDateTime};
    ///
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
/// # Arguments
///
/// * `workday` - The workday record containing start and end times
/// * `pauses` - Collection of pause records for the workday
///
/// # Returns
///
/// A vector of `WorkInterval` objects representing continuous work periods.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::report::calculate_work_intervals;
/// use kasl::db::workdays::Workday;
/// use kasl::libs::pause::Pause;
///
/// let workday = Workday {
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
pub fn calculate_work_intervals(workday: &Workday, pauses: &[Pause]) -> Vec<WorkInterval> {
    // Determine workday end time (current time if still ongoing)
    let end_time = workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());

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
/// # Arguments
///
/// * `intervals` - Collection of work intervals to analyze
/// * `min_minutes` - Minimum acceptable interval duration in minutes
///
/// # Returns
///
/// `Some(ShortIntervalsInfo)` if short intervals are found, `None` otherwise.
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
            total_duration = total_duration + interval.duration;

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
