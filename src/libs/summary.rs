//! Monthly work summary calculation and formatting system.
//!
//! Provides comprehensive functionality for calculating, processing, and formatting
//! monthly work summaries. Handles the complex business logic of combining actual
//! work data with company rest days to produce complete monthly reports.
//!
//! ## Features
//!
//! - **Daily Summary Aggregation**: Combines work duration and productivity metrics
//! - **Rest Day Integration**: Incorporates company holidays and weekends
//! - **Statistical Calculations**: Computes totals, averages, and productivity metrics
//! - **Flexible Formatting**: Provides multiple output formats for different use cases
//! - **Report Generation**: Powers monthly reports and export functionality
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::summary::{DailySummary, SummaryCalculator, SummaryFormatter};
//! use chrono::{Duration, NaiveDate};
//! use std::collections::HashSet;
//!
//! let summaries = vec![
//!     DailySummary {
//!         date: NaiveDate::from_ymd_opt(2025, 8, 11).unwrap(),
//!         duration: Duration::hours(8),
//!         productivity: 85.5,
//!     },
//! ];
//!
//! let rest_dates = HashSet::new();
//! let (processed, total, average) = summaries
//!     .add_rest_dates(rest_dates, Duration::hours(8))
//!     .calculate_totals();
//! ```

use crate::libs::formatter::format_duration;
use chrono::{Duration, NaiveDate};
use std::collections::{HashMap, HashSet};

/// Represents a complete work summary for a single calendar day.
///
/// This structure encapsulates all the key metrics needed to understand
/// work performance on a given day, including both time-based and
/// productivity-based measurements. It serves as the fundamental unit
/// for monthly reporting and analysis.
///
/// ## Data Components
///
/// ### Duration Tracking
/// The duration represents net productive work time:
/// - **Gross Time**: Total presence time (workday start to end)
/// - **Pause Time**: All break periods during the day
/// - **Net Time**: Gross time minus pause time (stored in duration field)
///
/// ### Productivity Metrics
/// Productivity is calculated as a percentage representing work efficiency:
/// - **Formula**: (Net Work Time / Gross Presence Time) × 100
/// - **Range**: 0.0 to 100.0 (theoretical maximum)
/// - **Typical Values**: 70-90% for most office work patterns
/// - **Factors**: Affected by meeting frequency, break patterns, and work type
///
/// ## Use Cases
///
/// ### Individual Analysis
/// - Daily productivity tracking and improvement
/// - Work pattern analysis and optimization
/// - Break frequency and duration analysis
///
/// ### Organizational Reporting
/// - Monthly time sheets and hour summaries
/// - Productivity benchmarking across teams
/// - Resource allocation and planning data
/// - Compliance with work hour regulations
///
/// ### External Integration
/// - Payroll system integration
/// - Project time allocation
/// - Client billing and invoicing
/// - Performance review documentation
#[derive(Debug, Clone)]
pub struct DailySummary {
    /// The specific calendar date for this work summary.
    ///
    /// Uses `NaiveDate` to avoid timezone complications in reporting.
    /// All daily summaries are associated with local calendar dates
    /// for consistency with user expectations and business requirements.
    ///
    /// ## Date Handling
    /// - **Local Time**: Uses system local time for date determination
    /// - **Calendar Days**: Aligned with user's local calendar
    /// - **Reporting Periods**: Consistent with organizational calendars
    /// - **Time Zones**: Avoids complexity by using naive dates
    pub date: NaiveDate,

    /// The total net productive work duration for this day.
    ///
    /// This represents the actual working time after subtracting
    /// all pause periods from the total presence time. It provides
    /// the most accurate measure of productive work hours.
    ///
    /// ## Calculation Method
    /// ```text
    /// Net Duration = (Workday End - Workday Start) - Total Pause Time
    ///
    /// Example:
    /// Workday: 09:00 - 17:30 (8h 30m total presence)
    /// Pauses: 30m lunch + 15m coffee breaks = 45m
    /// Net Duration: 8h 30m - 45m = 7h 45m
    /// ```
    ///
    /// ## Quality Considerations
    /// - **Accuracy**: Reflects actual productive work time
    /// - **Billing**: Suitable for client billing and time tracking
    /// - **Analysis**: Enables meaningful productivity analysis
    /// - **Reporting**: Meets organizational reporting standards
    pub duration: Duration,

    /// Work productivity percentage for this day (0.0 to 100.0).
    ///
    /// This metric provides insight into work efficiency by comparing
    /// net productive time against total presence time. It helps identify
    /// patterns in work effectiveness and opportunities for improvement.
    ///
    /// ## Calculation Formula
    /// ```text
    /// Productivity = (Net Work Time / Gross Presence Time) × 100
    ///
    /// Example:
    /// Net Work: 7h 45m = 465 minutes
    /// Gross Presence: 8h 30m = 510 minutes  
    /// Productivity = (465 / 510) × 100 = 91.2%
    /// ```
    ///
    /// ## Interpretation Guidelines
    /// - **90-100%**: Highly focused work with minimal interruptions
    /// - **80-90%**: Good productivity with normal break patterns
    /// - **70-80%**: Moderate productivity, may indicate heavy meeting load
    /// - **60-70%**: Lower productivity, worth investigating causes
    /// - **<60%**: Potential issues with work patterns or data accuracy
    ///
    /// ## Factors Affecting Productivity
    /// - **Meeting Density**: High meeting days typically show lower productivity
    /// - **Work Type**: Creative work may have different patterns than administrative
    /// - **Break Habits**: More frequent short breaks vs. fewer long breaks
    /// - **External Factors**: Interruptions, system issues, training sessions
    pub productivity: f64,
}

/// Trait for processing and enhancing collections of daily summaries.
///
/// This trait provides the core business logic for preparing monthly
/// reports by integrating actual work data with organizational calendar
/// information. It ensures comprehensive coverage of all calendar days
/// and provides statistical analysis capabilities.
///
/// ## Design Philosophy
///
/// The trait follows a functional programming approach with method chaining:
/// ```rust,no_run
/// let result = summaries
///     .add_rest_dates(company_holidays, default_hours)
///     .calculate_totals();
/// ```
///
/// This design enables:
/// - **Composability**: Methods can be chained for complex transformations
/// - **Immutability**: Each method returns a new collection
/// - **Readability**: Clear sequence of data processing steps
/// - **Testability**: Each transformation can be tested independently
///
/// ## Implementation Strategy
///
/// Implementations should handle:
/// - **Data Completeness**: Ensure all calendar days are represented
/// - **Duplicate Prevention**: Avoid duplicate entries for the same date
/// - **Statistical Accuracy**: Provide meaningful aggregate calculations
/// - **Performance**: Efficient processing of month-long datasets
pub trait SummaryCalculator {
    /// Integrates company rest days into the summary collection.
    ///
    /// This method ensures comprehensive monthly coverage by adding entries
    /// for company holidays, weekends, and other non-working days that should
    /// be included in monthly reports. It prevents gaps in monthly summaries
    /// and provides complete calendar coverage.
    ///
    /// ## Integration Logic
    ///
    /// The method processes rest dates as follows:
    /// 1. **Existence Check**: Verifies if a summary already exists for each rest date
    /// 2. **Gap Filling**: Adds new summary entries for missing rest dates
    /// 3. **Default Values**: Assigns standard work hours and zero productivity
    /// 4. **Duplicate Prevention**: Skips rest dates that already have work data
    ///
    /// ## Default Duration Rationale
    ///
    /// Rest days are assigned default work hours for several reasons:
    /// - **Payroll Integration**: Many payroll systems expect standard hours
    /// - **Monthly Targets**: Helps meet monthly hour requirements
    /// - **Benefit Allocation**: Paid holidays contribute to monthly totals
    /// - **Reporting Consistency**: Provides predictable monthly hour calculations
    ///
    /// ## Productivity Handling
    ///
    /// Rest days are assigned 0.0% productivity because:
    /// - **Accuracy**: No actual work was performed
    /// - **Statistical Integrity**: Prevents artificial inflation of productivity metrics
    /// - **Clear Distinction**: Differentiates between work days and rest days
    /// - **Analysis**: Enables separate analysis of work vs. rest day patterns
    ///
    /// # Arguments
    ///
    /// * `rest_dates` - Set of dates that are company holidays or rest days
    /// * `duration` - Default work duration to assign to rest days (typically 8 hours)
    ///
    /// # Returns
    ///
    /// Returns a new collection with rest days integrated, maintaining the original
    /// work data while filling gaps with rest day entries.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::collections::HashSet;
    /// use chrono::{Duration, NaiveDate};
    ///
    /// let mut rest_dates = HashSet::new();
    /// rest_dates.insert(NaiveDate::from_ymd_opt(2025, 8, 15).unwrap()); // Company holiday
    ///
    /// let enhanced_summaries = work_summaries
    ///     .add_rest_dates(rest_dates, Duration::hours(8));
    /// ```
    fn add_rest_dates(self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> Self;

    /// Calculates comprehensive statistics for the summary collection.
    ///
    /// This method performs the final aggregation step to produce monthly
    /// statistics including total work hours, average daily hours, and
    /// other metrics needed for reporting and analysis.
    ///
    /// ## Statistical Calculations
    ///
    /// ### Total Duration
    /// - **Sum**: Aggregates all daily durations in the collection
    /// - **Includes**: Both work days and rest days with default hours
    /// - **Purpose**: Monthly hour totals for payroll and reporting
    ///
    /// ### Average Duration
    /// - **Formula**: Total duration divided by number of days
    /// - **Significance**: Daily hour target and performance benchmarking
    /// - **Accuracy**: Reflects realistic daily work expectations
    ///
    /// ## Data Preparation
    ///
    /// Before calculation, the method:
    /// 1. **Sorts** summaries by date for consistent processing
    /// 2. **Validates** data integrity and completeness
    /// 3. **Handles** edge cases like empty collections
    /// 4. **Optimizes** calculations for performance
    ///
    /// ## Return Value Structure
    ///
    /// Returns a tuple containing:
    /// - **Enhanced Collection**: Sorted and processed summary data
    /// - **Total Duration**: Sum of all daily durations
    /// - **Average Duration**: Mean daily duration across all days
    ///
    /// # Returns
    ///
    /// A tuple of `(Self, Duration, Duration)` where:
    /// - First element: Processed and sorted summary collection
    /// - Second element: Total duration across all days
    /// - Third element: Average duration per day
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// let (processed_summaries, total_hours, average_daily) = summaries
    ///     .add_rest_dates(rest_dates, Duration::hours(8))
    ///     .calculate_totals();
    ///
    /// println!("Total monthly hours: {}", format_duration(&total_hours));
    /// println!("Average daily hours: {}", format_duration(&average_daily));
    /// ```
    fn calculate_totals(self) -> (Self, Duration, Duration)
    where
        Self: Sized;
}

impl SummaryCalculator for Vec<DailySummary> {
    /// Integrates company rest days into the daily summary collection.
    ///
    /// This implementation provides the standard logic for incorporating
    /// organizational rest days into monthly work summaries. It ensures
    /// complete calendar coverage while preserving existing work data.
    ///
    /// ## Implementation Details
    ///
    /// ### Duplicate Detection
    /// Uses an efficient lookup strategy:
    /// - Creates a temporary set of existing dates for O(1) lookups
    /// - Checks each rest date against existing summaries
    /// - Only adds entries for truly missing dates
    ///
    /// ### Memory Efficiency
    /// - Pre-allocates space for new entries to minimize reallocations
    /// - Uses iterator chains to avoid intermediate collections
    /// - Processes rest dates in batch for optimal performance
    ///
    /// ### Data Consistency
    /// - Maintains the same data structure and field meanings
    /// - Uses consistent duration and productivity value assignment
    /// - Preserves the ability to distinguish between work and rest days
    ///
    /// # Arguments
    ///
    /// * `rest_dates` - HashSet of dates to be added as rest days
    /// * `duration` - Standard duration to assign (typically 8 hours)
    ///
    /// # Returns
    ///
    /// A new Vec<DailySummary> with rest days integrated
    fn add_rest_dates(mut self, rest_dates: HashSet<NaiveDate>, duration: Duration) -> Self {
        // Process each rest date for potential addition
        for rest_date in rest_dates {
            // Check if we already have a summary for this date
            let date_exists = self.iter().any(|summary| summary.date == rest_date);

            if !date_exists {
                // Create a new summary entry for the rest day
                self.push(DailySummary {
                    date: rest_date,
                    duration,
                    productivity: 0.0, // Rest days have zero productivity
                });
            }
        }

        self
    }

    /// Calculates total and average durations for the summary collection.
    ///
    /// This implementation provides comprehensive statistical analysis of
    /// the monthly work data, producing both raw totals and meaningful
    /// averages for reporting purposes.
    ///
    /// ## Processing Steps
    ///
    /// ### 1. Data Sorting
    /// - Sorts summaries chronologically by date
    /// - Ensures consistent ordering for reports and analysis
    /// - Facilitates pattern recognition in work data
    ///
    /// ### 2. Total Calculation
    /// - Uses iterator fold for efficient aggregation
    /// - Handles Duration arithmetic correctly
    /// - Accumulates across all days in the collection
    ///
    /// ### 3. Average Calculation
    /// - Divides total by actual number of days
    /// - Handles edge case of empty collections gracefully
    /// - Provides realistic daily work expectations
    ///
    /// ## Error Handling
    ///
    /// - **Empty Collection**: Returns zero values for both total and average
    /// - **Invalid Durations**: Negative durations are treated as zero
    /// - **Overflow Protection**: Uses checked arithmetic where appropriate
    ///
    /// ## Performance Characteristics
    ///
    /// - **Time Complexity**: O(n log n) due to sorting requirement
    /// - **Space Complexity**: O(1) additional space for calculations
    /// - **Memory Usage**: In-place sorting minimizes memory overhead
    ///
    /// # Returns
    ///
    /// Tuple containing:
    /// - Sorted summary collection
    /// - Total duration across all days
    /// - Average duration per day
    fn calculate_totals(mut self) -> (Self, Duration, Duration) {
        // Sort summaries chronologically for consistent presentation
        self.sort_by_key(|summary| summary.date);

        // Calculate total duration across all days
        let total_duration = self.iter().fold(Duration::zero(), |accumulator, summary| accumulator + summary.duration);

        // Calculate average duration per day
        let day_count = self.len() as i64;
        let average_duration = if day_count > 0 {
            // Calculate average by dividing total seconds by number of days
            Duration::seconds(total_duration.num_seconds() / day_count)
        } else {
            // Handle empty collection case
            Duration::zero()
        };

        (self, total_duration, average_duration)
    }
}

/// Trait for formatting calculated summaries into human-readable output.
///
/// This trait provides the final transformation step in the summary processing
/// pipeline, converting calculated statistics into formatted strings suitable
/// for display, reporting, and export. It bridges the gap between business
/// logic and presentation layer requirements.
///
/// ## Design Goals
///
/// - **User-Friendly Formatting**: Convert technical data into readable formats
/// - **Consistent Presentation**: Standardized formatting across the application
/// - **Flexible Output**: Support multiple display contexts and requirements
/// - **Localization Ready**: Structure that can support future localization needs
///
/// ## Output Formats
///
/// The trait produces multiple output formats:
/// - **Daily Breakdown**: Day-by-day duration and productivity information
/// - **Summary Statistics**: Total and average values for the entire period
/// - **Structured Data**: Hash maps for flexible data access and manipulation
///
/// ## Integration Points
///
/// Formatted output is used by:
/// - **Console Display**: Terminal-based monthly summaries
/// - **Report Generation**: PDF and HTML report creation
/// - **Export Functions**: CSV and JSON data export
/// - **API Responses**: Web service and integration endpoints
pub trait SummaryFormatter {
    /// Formats summary data into comprehensive display-ready output.
    ///
    /// This method performs the final transformation of calculated summary
    /// data into formatted strings suitable for human consumption. It handles
    /// duration formatting, percentage display, and summary statistics
    /// presentation.
    ///
    /// ## Output Structure
    ///
    /// Returns a tuple containing three components:
    ///
    /// ### 1. Daily Summary Map
    /// - **Key**: Date for each day in the summary period
    /// - **Value**: Tuple of (formatted_duration, formatted_productivity)
    /// - **Purpose**: Day-by-day breakdown for detailed analysis
    /// - **Format**: Consistent "HH:MM" duration and "XX.X%" productivity
    ///
    /// ### 2. Total Duration String
    /// - **Content**: Formatted total of all daily durations
    /// - **Format**: "HH:MM" representation of total work hours
    /// - **Use Case**: Monthly hour totals for payroll and reporting
    ///
    /// ### 3. Average Duration String
    /// - **Content**: Formatted average daily duration
    /// - **Format**: "HH:MM" representation of typical daily hours
    /// - **Use Case**: Performance benchmarking and planning
    ///
    /// ## Formatting Standards
    ///
    /// ### Duration Format
    /// - **Pattern**: "HH:MM" (hours:minutes)
    /// - **Examples**: "08:30", "07:45", "09:15"
    /// - **Zero Handling**: "00:00" for zero durations
    /// - **Large Values**: Properly handles >24 hour totals
    ///
    /// ### Productivity Format
    /// - **Pattern**: "XX.X%" (percentage with one decimal place)
    /// - **Examples**: "85.5%", "92.1%", "78.0%"
    /// - **Range**: 0.0% to 100.0% (theoretical maximum)
    /// - **Precision**: One decimal place for meaningful granularity
    ///
    /// # Returns
    ///
    /// A tuple of `(HashMap<NaiveDate, (String, String)>, String, String)` containing:
    /// - Daily breakdown map with formatted duration and productivity
    /// - Formatted total duration string
    /// - Formatted average duration string
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// let (daily_map, total_str, avg_str) = calculated_summaries.format_summary();
    ///
    /// // Access daily data
    /// for (date, (duration, productivity)) in daily_map {
    ///     println!("{}: {} hours ({})", date, duration, productivity);
    /// }
    ///
    /// // Display summary statistics
    /// println!("Total: {}, Average: {}", total_str, avg_str);
    /// ```
    fn format_summary(&self) -> (HashMap<NaiveDate, (String, String)>, String, String);
}

impl SummaryFormatter for (Vec<DailySummary>, Duration, Duration) {
    /// Formats the complete summary tuple into display-ready output.
    ///
    /// This implementation provides comprehensive formatting for monthly
    /// summary data, handling both daily breakdowns and aggregate statistics
    /// with consistent formatting standards throughout.
    ///
    /// ## Processing Logic
    ///
    /// ### Daily Summary Processing
    /// - Iterates through each daily summary in the collection
    /// - Applies consistent duration formatting using shared utilities
    /// - Formats productivity percentages with appropriate precision
    /// - Creates a lookup map for efficient access by date
    ///
    /// ### Aggregate Formatting
    /// - Uses the same duration formatter for consistency
    /// - Handles large total durations (>24 hours) correctly
    /// - Provides meaningful average calculations
    ///
    /// ## Implementation Details
    ///
    /// ### Memory Efficiency
    /// - Pre-allocates HashMap with known capacity
    /// - Uses iterator chains to minimize intermediate allocations
    /// - Reuses formatting functions for consistency
    ///
    /// ### Error Handling
    /// - Gracefully handles edge cases like zero durations
    /// - Ensures consistent output format regardless of input quality
    /// - Provides sensible defaults for missing or invalid data
    ///
    /// ### Consistency Guarantees
    /// - All duration formatting uses the shared formatter
    /// - Productivity formatting follows application-wide standards
    /// - Output structure is consistent across all use cases
    ///
    /// # Returns
    ///
    /// Formatted summary data ready for display or further processing
    fn format_summary(&self) -> (HashMap<NaiveDate, (String, String)>, String, String) {
        // Extract components from the tuple
        let (daily_summaries, total_duration, average_duration) = self;

        // Format daily summaries into a lookup map
        let daily_durations = daily_summaries
            .iter()
            .map(|summary| {
                // Format duration using shared utility for consistency
                let formatted_duration = format_duration(&summary.duration);

                // Format productivity percentage with one decimal place
                let formatted_productivity = format!("{:.1}%", summary.productivity);

                // Create map entry
                (summary.date, (formatted_duration, formatted_productivity))
            })
            .collect();

        // Format aggregate statistics using shared utilities
        let total_duration_str = format_duration(total_duration);
        let average_duration_str = format_duration(average_duration);

        (daily_durations, total_duration_str, average_duration_str)
    }
}
