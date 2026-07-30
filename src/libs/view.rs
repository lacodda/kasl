//! Console display and table formatting system.
//!
//! Provides interface for rendering application data in well-formatted console tables.
//! Handles presentation layer for work reports, task lists, summaries, templates, and tags.
//!
//! ## Features
//!
//! - **Structured Data Display**: Converts complex data structures into readable tables
//! - **Consistent Formatting**: Maintains uniform appearance across all table types
//! - **Report Visualization**: Displays pre-calculated productivity and work metrics
//! - **Duration Formatting**: Handles time duration display in human-readable formats
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::view::View;
//!
//! View::tasks(&tasks)?;
//! View::report(&workday, &intervals, &filtered_duration, &productivity, &tasks)?;
//! View::sum(&summary_data)?;
//! ```

use super::task::Task;
use crate::db::templates::TaskTemplate;
use crate::db::workdays::Workday;
use crate::libs::formatter::{format_duration, terminal_cols, truncate_to_width};
use crate::libs::messages::Message;
use crate::libs::pause::Pause;
use crate::libs::report;
use crate::msg_print;
use anyhow::Result;
use chrono::{Duration, NaiveDate, TimeDelta};
use prettytable::{Cell, Row, Table, format, row};
use std::collections::HashMap;
use unicode_width::UnicodeWidthStr;

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
        let show_task_id = tasks.iter().any(|t| t.task_id.is_some_and(|id| id != 0));
        let show_comment = tasks.iter().any(|t| !t.comment.trim().is_empty());
        let show_tags = tasks.iter().any(|t| !t.tags.is_empty());

        let idx_width = tasks.len().to_string().width().max("#".width());
        let id_width = tasks.iter().map(|t| t.id.unwrap_or(0).to_string().width()).max().unwrap_or(1).max("ID".width());
        let task_id_width = if show_task_id {
            tasks
                .iter()
                .map(|t| t.task_id.unwrap_or(0).to_string().width())
                .max()
                .unwrap_or(1)
                .max("TASK ID".width())
        } else {
            0
        };
        let done_width = "DONE".width().max("100%".width());

        // prettytable cell format: `| content |` → 3 chars overhead per column + 1 outer border
        let mut num_cols = 4; // #, ID, NAME, DONE
        if show_task_id {
            num_cols += 1;
        }
        if show_comment {
            num_cols += 1;
        }
        if show_tags {
            num_cols += 1;
        }

        let mut fixed_content = idx_width + id_width + done_width;
        if show_task_id {
            fixed_content += task_id_width;
        }

        let frame_overhead = 3 * num_cols + 1;
        let mut flexible = terminal_cols().saturating_sub(frame_overhead + fixed_content);

        // Reserve a modest slice for optional text columns; NAME gets the rest.
        let tags_width = if show_tags {
            let width = (flexible / 5).clamp(8, 20);
            flexible = flexible.saturating_sub(width);
            width
        } else {
            0
        };
        let comment_width = if show_comment {
            let width = (flexible / 3).clamp(12, 40);
            flexible = flexible.saturating_sub(width);
            width
        } else {
            0
        };
        let name_width = flexible.max(12);

        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

        let mut titles = vec![Cell::new("#"), Cell::new("ID")];
        if show_task_id {
            titles.push(Cell::new("TASK ID"));
        }
        titles.push(Cell::new("NAME"));
        if show_comment {
            titles.push(Cell::new("COMMENT"));
        }
        titles.push(Cell::new("DONE"));
        if show_tags {
            titles.push(Cell::new("TAGS"));
        }
        table.set_titles(Row::new(titles));

        for (index, task) in tasks.iter().enumerate() {
            let mut cells = vec![Cell::new(&(index + 1).to_string()), Cell::new(&task.id.unwrap_or(0).to_string())];
            if show_task_id {
                cells.push(Cell::new(&task.task_id.unwrap_or(0).to_string()));
            }
            cells.push(Cell::new(&truncate_to_width(&task.name, name_width)));
            if show_comment {
                cells.push(Cell::new(&truncate_to_width(task.comment.trim(), comment_width)));
            }
            cells.push(Cell::new(&format!("{}%", task.completeness.unwrap_or(100))));
            if show_tags {
                let tags_str = task.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ");
                cells.push(Cell::new(&truncate_to_width(&tags_str, tags_width)));
            }
            table.add_row(Row::new(cells));
        }

        table.printstd();
        Ok(())
    }

    /// Displays a formatted daily work report using pre-calculated intervals.
    ///
    /// This method displays the core report data in a structured table format,
    /// including work intervals, total time, and productivity metrics calculated
    /// using the centralized Productivity module.
    ///
    /// ## Display Components
    ///
    /// 1. **Work Intervals**: Detailed breakdown of focused work periods
    /// 2. **Total Duration**: Sum of all work intervals (may be filtered)
    /// 3. **Productivity Percentage**: Calculated using comprehensive Productivity logic
    /// 4. **Associated Tasks**: Tasks completed during the workday for context
    ///
    /// The productivity value displayed here is calculated using the same centralized
    /// logic used throughout the application for consistency.
    ///
    /// # Arguments
    ///
    /// * `workday` - The workday record containing start/end times
    /// * `intervals` - Pre-calculated and optionally filtered work intervals for display
    /// * `filtered_duration` - Sum of displayed interval durations
    /// * `productivity` - Productivity percentage from centralized Productivity calculation
    /// * `tasks` - Tasks completed during the workday for context
    pub fn report(workday: &Workday, intervals: &[report::WorkInterval], filtered_duration: &TimeDelta, productivity: &f64, tasks: &[Task]) -> Result<()> {
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
        table.add_row(row!["TOTAL", "", "", format_duration(filtered_duration)]);
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

    /// Displays active Jira inbox items (pinned first, then by priority).
    pub fn jira_inbox(items: &[crate::db::jira_inbox::JiraInboxItem]) -> Result<()> {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["", "PRIORITY", "KEY", "STATUS", "SUMMARY"]);

        for item in items {
            let pin = if item.pinned { "★" } else { "" };
            table.add_row(row![
                pin,
                item.priority.as_deref().unwrap_or("—"),
                item.issue_key,
                item.status,
                item.summary,
            ]);
        }

        table.printstd();
        Ok(())
    }
}
