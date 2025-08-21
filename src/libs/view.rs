//! Console display and table formatting system.
//!
//! Provides interface for rendering application data in well-formatted console tables.
//! Handles presentation layer for work reports, task lists, summaries, templates, and tags.
//!
//! ## Features
//!
//! - **Structured Data Display**: Converts complex data structures into readable tables
//! - **Consistent Formatting**: Maintains uniform appearance across all table types
//! - **Productivity Analysis**: Calculates and displays work efficiency metrics
//! - **Duration Formatting**: Handles time duration display in human-readable formats
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::view::View;
//!
//! View::tasks(&tasks)?;
//! View::report(&workday, &long_breaks, &all_pauses, &tasks)?;
//! View::sum(&summary_data)?;
//! ```

use super::task::Task;
use crate::db::templates::TaskTemplate;
use crate::db::workdays::Workday;
use crate::libs::formatter::format_duration;
use crate::libs::messages::Message;
use crate::libs::pause::Pause;
use crate::libs::report;
use crate::msg_print;
use anyhow::Result;
use chrono::{Duration, NaiveDate, TimeDelta};
use prettytable::{format, row, Table};
use std::collections::HashMap;

/// A utility struct for rendering application data to the console.
///
/// Serves as a namespace for various table rendering functions. All methods are static,
/// making it easy to call formatting functions without needing to instantiate the struct.
pub struct View {}

impl View {
    /// Displays a formatted table of tasks with comprehensive metadata.
    ///
    /// Renders a detailed table showing task information including identification numbers,
    /// names, completion status, comments, and associated tags.
    ///
    /// # Arguments
    ///
    /// * `tasks` - A slice of `Task` structs to display in the table
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful table rendering, or an error if
    /// the table cannot be displayed due to terminal or formatting issues.
    pub fn tasks(tasks: &[Task]) -> Result<()> {
        // Initialize table with clean formatting suitable for task data
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["#", "ID", "TASK ID", "NAME", "COMMENT", "COMPLETENESS", "TAGS"]);

        // Populate table with task data, adding sequential numbering
        for (index, task) in tasks.iter().enumerate() {
            // Format tags as a comma-separated string for compact display
            let tags_str = task.tags.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", ");

            table.add_row(row![
                index + 1,                                        // Human-friendly 1-based indexing for selection
                task.id.unwrap_or(0),                             // Database ID, showing 0 for new tasks
                task.task_id.unwrap_or(0),                        // External task ID (Jira, GitLab, etc.)
                task.name,                                        // Task title or summary
                task.comment,                                     // Additional notes or description
                format!("{}%", task.completeness.unwrap_or(100)), // Completion percentage with % symbol
                tags_str                                          // Formatted tag list
            ]);
        }

        // Render the table to standard output
        table.printstd();

        Ok(())
    }

    /// Displays a comprehensive daily work report with intervals, productivity, and tasks.
    ///
    /// Generates a detailed daily report that includes work time analysis, break patterns,
    /// productivity calculations, and associated tasks.
    ///
    /// The productivity calculation uses a sophisticated algorithm that distinguishes
    /// between different types of work interruptions:
    ///
    /// - **Long Breaks**: Significant interruptions (lunch, meetings) excluded from gross time
    /// - **Short Pauses**: Brief interruptions (bathroom, coffee) used for productivity calculation
    /// - **Net Duration**: Pure working time with all breaks excluded
    /// - **Productivity**: Percentage of gross time spent in active work
    ///
    /// ## Data Integration
    ///
    /// The method integrates data from multiple sources:
    /// - Workday records for start/end times
    /// - Pause detection for break analysis
    /// - Task records for completed work items
    /// - Interval calculation for time block analysis
    ///
    /// # Arguments
    ///
    /// * `workday` - The `Workday` record containing start/end times for the day
    /// * `long_breaks` - Filtered long breaks to display in the intervals table
    /// * `all_pauses` - Complete pause data for accurate productivity calculation
    /// * `tasks` - Slice of `Task` records completed during the day
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful report generation, or an error if
    /// formatting or display operations fail.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::view::View;
    ///
    /// View::report(&workday, &filtered_breaks, &all_pauses, &daily_tasks)?;
    /// ```
    pub fn report(workday: &Workday, long_breaks: &[Pause], all_pauses: &[Pause], tasks: &[Task]) -> Result<()> {
        // Display formatted report header with readable date
        msg_print!(Message::ReportHeader(workday.date.format("%B %-d, %Y").to_string()), true);

        // Calculate work time boundaries, using current time if workday is still active
        let end_time = workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());
        let gross_duration = end_time - workday.start;

        // Calculate total duration of long breaks for net time calculation
        // Long breaks are major interruptions that are excluded from productive time
        let daily_long_break_duration = long_breaks.iter().filter_map(|b| b.duration).fold(Duration::zero(), |acc, d| acc + d);

        // Net duration represents time available for work (excluding major breaks)
        let net_duration = gross_duration - daily_long_break_duration;

        // Calculate short pause duration for productivity analysis
        // This includes all pauses minus the long breaks already excluded
        let daily_short_pause_duration = all_pauses.iter().filter_map(|b| b.duration).fold(Duration::zero(), |acc, d| acc + d) - daily_long_break_duration;

        // Calculate work productivity using sophisticated algorithm
        let productivity = Self::calculate_productivity(&net_duration, &daily_short_pause_duration);

        // Generate work intervals using filtered breaks for clean display
        let intervals = report::calculate_work_intervals(workday, long_breaks);
        Self::render_report_with_intervals(workday, &intervals, all_pauses, tasks, &net_duration, productivity)
    }

    /// Displays a formatted daily work report using pre-calculated intervals.
    ///
    /// This version accepts pre-filtered work intervals, allowing for custom 
    /// filtering logic (e.g., removing short intervals) before display.
    ///
    /// # Arguments
    ///
    /// * `workday` - The workday record containing start/end times
    /// * `intervals` - Pre-calculated and optionally filtered work intervals
    /// * `all_pauses` - Complete pause record for accurate productivity analysis  
    /// * `tasks` - Tasks completed during the workday for context
    pub fn report_with_intervals(
        workday: &Workday,
        intervals: &[report::WorkInterval], 
        all_pauses: &[Pause],
        tasks: &[Task]
    ) -> Result<()> {
        // Calculate work time boundaries and productivity with given intervals
        let end_time = workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());
        let _gross_duration = end_time - workday.start;
        
        // Calculate filtered duration based on provided intervals
        let filtered_duration = intervals.iter()
            .fold(Duration::zero(), |acc, interval| acc + interval.duration);
            
        // Calculate all pause duration for productivity analysis
        let total_pause_duration = all_pauses.iter()
            .filter_map(|b| b.duration)
            .fold(Duration::zero(), |acc, d| acc + d);
            
        let productivity = Self::calculate_productivity(&filtered_duration, &total_pause_duration);
        
        Self::render_report_with_intervals(workday, intervals, all_pauses, tasks, &filtered_duration, productivity)
    }

    /// Internal method to render the actual report table and content.
    fn render_report_with_intervals(
        workday: &Workday,
        intervals: &[report::WorkInterval],
        _all_pauses: &[Pause], 
        tasks: &[Task],
        net_duration: &Duration,
        productivity: f64
    ) -> Result<()> {
        // Display formatted report header with readable date
        msg_print!(Message::ReportHeader(workday.date.format("%B %-d, %Y").to_string()), true);

        // Create and populate the work intervals table
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "START", "END", "DURATION"]);

        // Add each work interval as a table row with formatted times
        for (index, interval) in intervals.iter().enumerate() {
            table.add_row(row![
                index + 1,                           // Sequential numbering for easy reference
                interval.start.format("%H:%M"),      // Start time in HH:MM format
                interval.end.format("%H:%M"),        // End time in HH:MM format
                format_duration(&interval.duration)  // Human-readable duration
            ]);
        }

        // Add summary rows with total time and productivity metrics
        table.add_empty_row(); // Visual separator before summary
        table.add_row(row!["TOTAL", "", "", format_duration(&net_duration)]);
        table.add_row(row!["PRODUCTIVITY", "", "", format!("{:.1}%", productivity)]);

        // Render the intervals table to console
        table.printstd();

        // Display associated tasks if any were completed during the day
        if !tasks.is_empty() {
            msg_print!(Message::TasksHeader, true);
            Self::tasks(tasks)?;
        }

        Ok(())
    }

    /// Displays a monthly summary of working hours with daily breakdowns.
    ///
    /// This method renders a comprehensive monthly view that shows daily work
    /// patterns, totals, and averages. It provides both detailed daily data
    /// and aggregate statistics to help users understand their work patterns
    /// over the entire month.
    ///
    /// ## Summary Structure
    ///
    /// The monthly summary includes:
    /// - **Daily Breakdown**: Each day with date, hours worked, and workday status
    /// - **Total Hours**: Cumulative time worked across all days in the month
    /// - **Average Hours**: Mean daily working time for better pattern analysis
    /// - **Work Days**: Count of days with recorded work activity
    ///
    /// ## Data Interpretation
    ///
    /// - **Workday Hours**: Actual time recorded for productive work days
    /// - **Rest Day Hours**: Default hours applied to weekends and holidays
    /// - **Missing Days**: Days without any recorded activity (shown as 0:00)
    ///
    /// # Arguments
    ///
    /// * `summary_data` - A tuple containing:
    ///   - `HashMap<NaiveDate, (String, String)>`: Daily durations and productivity data
    ///   - `String`: Total duration for the entire month
    ///   - `String`: Average daily duration across all days
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful summary display, or an error if
    /// table formatting or rendering fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::view::View;
    /// use std::collections::HashMap;
    ///
    /// let summary_data = (daily_map, total_hours, average_hours);
    /// View::sum(&summary_data)?;
    /// ```
    pub fn sum((daily_durations, total_duration, average_duration): &(HashMap<NaiveDate, (String, String)>, String, String)) -> Result<()> {
        // Initialize table with appropriate formatting for summary data
        let mut table: Table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["DATE", "HOURS", "PRODUCTIVITY"]);

        // Sort dates chronologically for logical display order
        let mut sorted_dates: Vec<&NaiveDate> = daily_durations.keys().collect();
        sorted_dates.sort();

        // Add each day's data as a table row
        for date in sorted_dates {
            if let Some((duration, productivity)) = daily_durations.get(date) {
                table.add_row(row![
                    date.format("%Y-%m-%d"), // ISO date format for consistency
                    duration,                // Formatted duration string
                    productivity             // Productivity percentage or status
                ]);
            }
        }

        // Add summary statistics with visual separation
        table.add_empty_row(); // Visual separator before totals
        table.add_row(row!["TOTAL", total_duration, ""]);
        table.add_row(row!["AVERAGE", average_duration, ""]);

        // Render the summary table to console
        table.printstd();
        Ok(())
    }

    /// Displays a table of pauses for a given day with total pause time.
    ///
    /// # Arguments
    /// * `pauses` - A slice of `Pause` records to display.
    /// * `total_pause_time` - The total duration of all pauses.
    ///
    /// # Returns
    /// A `Result` indicating success.
    pub fn pauses(pauses: &[Pause], total_pause_time: Duration) -> Result<()> {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "START", "END", "DURATION"]);

        for (i, b) in pauses.iter().enumerate() {
            table.add_row(row![
                i + 1,
                b.start.format("%H:%M"),
                b.end.map(|t| t.format("%H:%M").to_string()).unwrap_or_else(|| "-".to_string()),
                b.duration
                    .map(|duration: TimeDelta| format_duration(&duration))
                    .unwrap_or_else(|| "--:--".to_string())
            ]);
        }

        // Add total row
        if !pauses.is_empty() {
            table.add_empty_row();
            table.add_row(row!["TOTAL", "", "", format_duration(&total_pause_time)]);
        }

        table.printstd();
        Ok(())
    }

    /// Calculates work productivity based on net working time and pause duration.
    ///
    /// This method implements a sophisticated productivity calculation that provides
    /// meaningful insights into work efficiency. It distinguishes between time spent
    /// actively working and time spent on brief interruptions or non-productive activities.
    ///
    /// ## Calculation Methodology
    ///
    /// The productivity calculation uses the following approach:
    ///
    /// 1. **Base Time**: Gross work time with long breaks already excluded
    /// 2. **Active Time**: Base time minus short pauses and brief interruptions
    /// 3. **Productivity**: Percentage of base time spent in active work
    ///
    /// ## Mathematical Formula
    ///
    /// ```
    /// Productivity = (Net Working Time / Gross Working Time) × 100%
    /// ```
    ///
    /// Where:
    /// - **Net Working Time** = Gross Time - Short Pauses
    /// - **Gross Working Time** = Total Presence - Long Breaks
    ///
    /// ## Edge Case Handling
    ///
    /// The method handles several edge cases gracefully:
    /// - **Zero Base Time**: Returns 0% productivity to avoid division by zero
    /// - **Negative Net Time**: Uses safe subtraction with zero fallback
    /// - **Invalid Ranges**: Clamps results to valid 0-100% range
    ///
    /// ## Productivity Insights
    ///
    /// This metric determines the proportion of time truly spent on active work
    /// relative to the total time available for work, where only long breaks are excluded
    /// from the overall presence time.
    ///
    /// # Arguments
    ///
    /// * `gross_work_time_minus_long_breaks` - The total time spent at work, with only long breaks already excluded.
    ///                                         This duration still includes short, minor pauses.
    /// * `daily_short_pause_duration` - The total duration of short, minor pauses (e.g., quick coffee breaks, brief distractions).
    ///
    /// # Returns
    ///
    /// The percentage of time spent in actual productive work (0.0 - 100.0).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use chrono::Duration;
    /// use kasl::libs::view::View;
    ///
    /// let work_time = Duration::hours(8);
    /// let short_pauses = Duration::minutes(30);
    /// let productivity = View::calculate_productivity(&work_time, &short_pauses);
    /// // Returns approximately 93.75% (7.5 hours / 8 hours)
    /// ```
    fn calculate_productivity(gross_work_time_minus_long_breaks: &Duration, daily_short_pause_duration: &Duration) -> f64 {
        // Calculate the truly "net" working time by subtracting short pauses from
        // the time already adjusted for long breaks.
        // This represents the time exclusively dedicated to productive tasks.
        let net_working_duration = gross_work_time_minus_long_breaks.checked_sub(&daily_short_pause_duration).unwrap_or_else(|| {
            // Handle cases where subtraction might result in a negative duration (e.g., if short pauses > gross_work_time_minus_long_breaks).
            // Returning Duration::zero() is a safe fallback to prevent panics and ensure a 0% productivity in such edge cases.
            Duration::zero()
        });

        // If the base time for calculation (gross_work_time_minus_long_breaks) is zero,
        // productivity is 0% to avoid division by zero.
        if gross_work_time_minus_long_breaks.num_seconds() == 0 {
            return 0.0;
        }

        // Calculate productivity as (net working duration / gross work time minus long breaks) * 100.
        // This gives the percentage of time truly spent productively out of the time "on duty"
        // (excluding only major breaks).
        let productivity = (net_working_duration.num_seconds() as f64 / gross_work_time_minus_long_breaks.num_seconds() as f64) * 100.0;

        // Ensure the resulting percentage is within the valid range [0.0, 100.0]
        productivity.max(0.0).min(100.0)
    }

    /// Displays a formatted table of task templates for reusable task creation.
    ///
    /// This method renders a comprehensive view of all available task templates,
    /// showing their configuration and usage information. Templates provide a
    /// convenient way to create commonly used tasks with pre-filled parameters.
    ///
    /// ## Template Information
    ///
    /// The table displays essential template metadata:
    /// - **Template Name**: Unique identifier for template selection
    /// - **Task Name**: Default task title that will be used
    /// - **Comment**: Pre-configured task description or notes
    /// - **Completeness**: Default completion percentage for new tasks
    ///
    /// ## Usage Context
    ///
    /// Templates are particularly useful for:
    /// - Recurring tasks with standard parameters
    /// - Team workflows with consistent task structures
    /// - Quick task creation with minimal input required
    /// - Standardized task naming and completion patterns
    ///
    /// # Arguments
    ///
    /// * `templates` - A slice of `TaskTemplate` structs to display
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful table rendering, or an error if
    /// display operations fail.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::view::View;
    /// use kasl::db::templates::TaskTemplate;
    ///
    /// let templates = vec![/* template instances */];
    /// View::templates(&templates)?;
    /// ```
    pub fn templates(templates: &[TaskTemplate]) -> Result<()> {
        // Initialize table with clean formatting for template data
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["TEMPLATE NAME", "TASK NAME", "COMMENT", "COMPLETENESS"]);

        // Populate table with template information
        for template in templates {
            table.add_row(row![
                template.name,                         // Unique template identifier
                template.task_name,                    // Default task title
                template.comment,                      // Pre-configured description
                format!("{}%", template.completeness)  // Default completion with % symbol
            ]);
        }

        // Render the templates table to console
        table.printstd();
        Ok(())
    }

    /// Displays a formatted table of tags for task categorization and organization.
    ///
    /// This method provides a comprehensive view of all available tags that can
    /// be applied to tasks for organization and filtering purposes. The table
    /// shows both the functional and visual aspects of each tag.
    ///
    /// ## Tag Information
    ///
    /// The table displays key tag metadata:
    /// - **ID**: Unique database identifier for programmatic reference
    /// - **NAME**: Human-readable tag name used for categorization
    /// - **COLOR**: Optional color coding for visual organization (if supported)
    ///
    /// ## Organizational Benefits
    ///
    /// Tags provide several organizational advantages:
    /// - **Categorization**: Group related tasks by project, priority, or type
    /// - **Filtering**: Quickly find tasks based on specific criteria
    /// - **Visual Organization**: Color coding for rapid visual identification
    /// - **Reporting**: Generate reports filtered by specific tag categories
    ///
    /// ## Color Display
    ///
    /// Colors are displayed as text values (hex codes, names, etc.) since
    /// terminal color support varies. A dash (-) indicates no color assigned.
    ///
    /// # Arguments
    ///
    /// * `tags` - A slice of `Tag` structs to display in the table
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful table rendering, or an error if
    /// display operations fail.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::view::View;
    /// use kasl::db::tags::Tag;
    ///
    /// let tags = vec![/* tag instances */];
    /// View::tags(&tags)?;
    /// ```
    pub fn tags(tags: &[crate::db::tags::Tag]) -> Result<()> {
        // Initialize table with appropriate formatting for tag data
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "NAME", "COLOR"]);

        // Populate table with tag information
        for tag in tags {
            table.add_row(row![
                tag.id.unwrap_or(0),                 // Database ID, showing 0 for new tags
                tag.name,                            // Human-readable tag name
                tag.color.as_deref().unwrap_or("-")  // Color value or dash if none
            ]);
        }

        // Render the tags table to console
        table.printstd();
        Ok(())
    }
}
