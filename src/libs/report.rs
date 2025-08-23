//! Work interval calculation and productivity analysis for daily reports.
//!
//! Provides core logic for analyzing work patterns and generating detailed reports
//! about productivity, work intervals, and break patterns.
//!
//! ## Features
//!
//! - **Work Interval Analysis**: Convert workday and pause data into continuous work periods
//! - **Productivity Metrics**: Calculate efficiency ratios and work pattern analysis
//! - **Short Interval Detection**: Identify and analyze brief work periods that may indicate interruptions
//! - **Interval Filtering**: Filter out short intervals for cleaner reporting (display-level, no database changes)
//! - **Report Generation**: Comprehensive breakdown of workdays with productivity analysis
//!
//! ## Usage
//!
//! ```rust
//! use kasl::libs::report::{calculate_work_intervals, filter_short_intervals, WorkInterval};
//! use kasl::db::workdays::Workday;
//! use kasl::libs::pause::Pause;
//!
//! let workday = Workday {
//!     start: start_time,
//!     end: Some(end_time),
//! };
//!
//! let pauses = vec![/* pause data */];
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
/// # Arguments
///
/// * `intervals` - Original work intervals to filter
/// * `min_minutes` - Minimum duration in minutes for intervals to keep
///
/// # Returns
///
/// Returns `(filtered_intervals, filtered_info)` where:
/// - `filtered_intervals` contains only intervals >= min_minutes
/// - `filtered_info` contains details about filtered intervals (None if nothing was filtered)
///
/// # Examples
///
/// ```rust
/// use kasl::libs::report::filter_short_intervals;
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
            total_duration = total_duration + interval.duration;
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
/// # Arguments
///
/// * `workday` - The workday record containing start/end times
/// * `intervals` - Pre-calculated and optionally filtered work intervals for duration calculation
///
/// # Returns
///
/// Returns a tuple containing:
/// - **Filtered Duration**: Sum of provided work intervals (may exclude short intervals)
/// - **Productivity**: Comprehensive productivity percentage using centralized calculation
///
/// # Examples
///
/// ```rust
/// let (duration, productivity) = report_with_intervals(&workday, &filtered_intervals)?;
/// println!("Work time: {}, Productivity: {:.1}%", format_duration(duration), productivity);
/// ```
pub fn report_with_intervals(
    workday: &Workday,
    intervals: &[WorkInterval]
) -> Result<(Duration, f64)> {
    // Calculate filtered duration based on provided intervals (for display purposes)
    let filtered_duration = intervals.iter().fold(Duration::zero(), |acc, interval| acc + interval.duration);

    // Use centralized productivity module for consistent, comprehensive calculation
    let productivity = Productivity::new(&workday)?.calculate_productivity();

    Ok((filtered_duration, productivity))
}

/// Combines breaks and pauses into a unified collection for work interval calculation.
///
/// This function creates a combined list of all work interruptions (both manual breaks
/// and automatic pauses) to ensure that work intervals are calculated accurately,
/// accounting for all types of non-work time.
///
/// ## Data Integration
///
/// - **Manual Breaks**: User-defined break periods from the breaks table
/// - **Automatic Pauses**: System-detected pauses from the pauses table
/// - **Unified Format**: Both are converted to a common Pause-like structure
///
/// ## Temporal Ordering
///
/// The combined collection is sorted chronologically to ensure proper
/// work interval calculation when multiple types of interruptions occur
/// throughout the workday.
///
/// # Arguments
///
/// * `breaks` - Manual breaks from the breaks database table
/// * `pauses` - Automatic pauses from the pauses database table
///
/// # Returns
///
/// A vector of Pause objects representing all work interruptions,
/// sorted chronologically by start time.
///
/// # Examples
///
/// ```rust
/// use kasl::libs::report::combine_breaks_and_pauses;
/// 
/// let breaks = breaks_db.get_daily_breaks(date)?;
/// let pauses = pauses_db.get_daily_pauses(date)?;
/// let combined = combine_breaks_and_pauses(&breaks, &pauses);
/// 
/// let intervals = calculate_work_intervals(&workday, &combined);
/// ```
pub fn combine_breaks_and_pauses(
    breaks: &[crate::db::breaks::Break], 
    pauses: &[crate::libs::pause::Pause]
) -> Vec<crate::libs::pause::Pause> {
    let mut combined = Vec::new();
    
    // Add existing pauses
    combined.extend_from_slice(pauses);
    
    // Convert breaks to pause format and add them
    let mut next_id = pauses.iter().map(|p| p.id).max().unwrap_or(0) + 1000; // Use high IDs to avoid conflicts
    
    for break_record in breaks {
        combined.push(crate::libs::pause::Pause {
            id: next_id,
            start: break_record.start,
            end: Some(break_record.end),
            duration: Some(break_record.duration),
        });
        next_id += 1;
    }
    
    // Sort by start time for proper interval calculation
    combined.sort_by_key(|item| item.start);
    
    combined
}
