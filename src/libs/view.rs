//! Manages the display of data in formatted console tables.
//!
//! This module uses `prettytable-rs` to render various data structures,
//! such as tasks, work reports, and summaries, in a human-readable format.

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

/// A utility struct for rendering data to the console.
pub struct View {}

impl View {
    /// Displays a table of tasks.
    ///
    /// # Arguments
    /// * `tasks` - A slice of `Task` structs to display.
    ///
    /// # Returns
    /// A `Result` indicating success.
    pub fn tasks(tasks: &[Task]) -> Result<()> {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["#", "ID", "TASK ID", "NAME", "COMMENT", "COMPLETENESS"]);

        for (index, task) in tasks.iter().enumerate() {
            table.add_row(row![
                index + 1,
                task.id.unwrap_or(0),
                task.task_id.unwrap_or(0),
                task.name,
                task.comment,
                format!("{}%", task.completeness.unwrap_or(100))
            ]);
        }
        table.printstd();

        Ok(())
    }

    /// Displays a daily work report, including work intervals, pauses, and tasks.
    ///
    /// # Arguments
    /// * `workday` - The `Workday` record for the report.
    /// * `long_breaks` - Filtered long breaks to show in the intervals table.
    /// * `all_pauses` - All pauses for accurate productivity calculation.
    /// * `tasks` - A slice of `Task` records for the day.
    ///
    /// # Returns
    /// A `Result` indicating success.
    pub fn report(workday: &Workday, long_breaks: &[Pause], all_pauses: &[Pause], tasks: &[Task]) -> Result<()> {
        msg_print!(Message::ReportHeader(workday.date.format("%B %-d, %Y").to_string()), true);
        let end_time = workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());
        let gross_duration = end_time - workday.start;

        // Calculate total pause duration from long breaks only
        let daily_long_break_duration = long_breaks.iter().filter_map(|b| b.duration).fold(Duration::zero(), |acc, d| acc + d);
        let net_duration = gross_duration - daily_long_break_duration;

        // Сalculate total pause duration, excluding long breaks for accurate productivity
        let daily_short_pause_duration = all_pauses.iter().filter_map(|b| b.duration).fold(Duration::zero(), |acc, d| acc + d) - daily_long_break_duration;

        // Calculate work productivity using ALL pauses
        let productivity = Self::calculate_productivity(&net_duration, &daily_short_pause_duration);

        // Use filtered pauses for display
        let intervals = report::calculate_work_intervals(workday, long_breaks);

        // Create and populate the intervals table.
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "START", "END", "DURATION"]);

        for (index, interval) in intervals.iter().enumerate() {
            table.add_row(row![
                index + 1,
                interval.start.format("%H:%M"),
                interval.end.format("%H:%M"),
                format_duration(&interval.duration)
            ]);
        }
        table.add_empty_row();
        table.add_row(row!["TOTAL", "", "", format_duration(&net_duration)]);
        table.add_row(row!["PRODUCTIVITY", "", "", format!("{:.1}%", productivity)]);
        table.printstd();

        if !tasks.is_empty() {
            msg_print!(Message::TasksHeader, true);
            Self::tasks(tasks)?;
        }

        Ok(())
    }

    /// Displays a summary of working hours for a month.
    ///
    /// # Arguments
    /// * `summary_data` - A tuple containing daily durations, total duration, and average duration.
    ///
    /// # Returns
    /// A `Result` indicating success.
    pub fn sum((daily_durations, total_duration, average_duration): &(HashMap<NaiveDate, (String, String)>, String, String)) -> Result<()> {
        let mut table: Table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["DATE", "DURATION", "PRODUCTIVITY"]);

        let mut dates: Vec<&NaiveDate> = daily_durations.keys().collect();
        dates.sort();
        for date in dates {
            if let Some((duration_str, productivity)) = daily_durations.get(date) {
                table.add_row(row![date.format("%d.%m.%Y"), duration_str, productivity]);
            }
        }
        table.add_empty_row();
        table.add_row(row!["AVERAGE", average_duration]);
        table.add_row(row!["TOTAL", total_duration]);
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

    /// Calculates the percentage of actual productive working time.
    ///
    /// This metric determines the proportion of time truly spent on active work
    /// relative to the total time available for work, where only long breaks are excluded
    /// from the overall presence time.
    ///
    /// # Arguments
    /// * `gross_work_time_minus_long_breaks` - The total time spent at work, with only long breaks already excluded.
    ///                                         This duration still includes short, minor pauses.
    /// * `daily_short_pause_duration` - The total duration of short, minor pauses (e.g., quick coffee breaks, brief distractions).
    ///
    /// # Returns
    /// The percentage of time spent in actual productive work (0.0 - 100.0).
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

    /// Displays a table of task templates
    pub fn templates(templates: &[TaskTemplate]) -> Result<()> {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["TEMPLATE NAME", "TASK NAME", "COMMENT", "COMPLETENESS"]);

        for template in templates {
            table.add_row(row![template.name, template.task_name, template.comment, format!("{}%", template.completeness)]);
        }

        table.printstd();
        Ok(())
    }
}
