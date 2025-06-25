//! Manages the display of data in formatted console tables.
//!
//! This module uses `prettytable-rs` to render various data structures,
//! such as tasks, work reports, and summaries, in a human-readable format.

use super::task::Task;
use crate::db::workdays::Workday;
use crate::libs::formatter::format_duration;
use crate::libs::r#break::Break;
use chrono::{Duration, NaiveDate, TimeDelta};
use prettytable::{format, row, Table};
use std::collections::HashMap;
use std::error::Error;

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
    pub fn tasks(tasks: &[Task]) -> Result<(), Box<dyn Error>> {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "TASK ID", "NAME", "COMMENT", "COMPLETENESS"]);

        for (index, task) in tasks.iter().enumerate() {
            table.add_row(row![
                index + 1,
                task.task_id.unwrap_or(0),
                task.name,
                task.comment,
                task.completeness.unwrap_or(100)
            ]);
        }
        table.printstd();

        Ok(())
    }

    /// Displays a daily work report, including work intervals, breaks, and tasks.
    ///
    /// # Arguments
    /// * `workday` - The `Workday` record for the report.
    /// * `breaks` - A slice of `Break` records for the day.
    /// * `tasks` - A slice of `Task` records for the day.
    ///
    /// # Returns
    /// A `Result` indicating success.
    pub fn report(workday: &Workday, breaks: &[Break], tasks: &[Task]) -> Result<(), Box<dyn Error>> {
        println!("\nReport for {}", workday.date.format("%B %-d, %Y"));
        let end_time = workday.end.unwrap_or_else(|| chrono::Local::now().naive_local());
        let total_break_duration = breaks.iter().filter_map(|b| b.duration).fold(Duration::zero(), |acc, d| acc + d);
        let net_duration = (end_time - workday.start) - total_break_duration;

        // Calculate work intervals based on breaks.
        let mut intervals = vec![];
        let mut current_time = workday.start;
        let mut breaks_iter = breaks.iter().filter(|b| b.end.is_some()).collect::<Vec<_>>();
        breaks_iter.sort_by_key(|b| b.start);
        for b in breaks_iter {
            if current_time < b.start {
                intervals.push((current_time, b.start, b.start - current_time));
            }
            current_time = b.end.unwrap();
        }
        if current_time < end_time {
            intervals.push((current_time, end_time, end_time - current_time));
        }

        // Create and populate the intervals table.
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "START", "END", "DURATION"]);

        for (index, (start, end, duration)) in intervals.iter().enumerate() {
            table.add_row(row![index + 1, start.format("%H:%M"), end.format("%H:%M"), format_duration(duration)]);
        }
        table.add_empty_row();
        table.add_row(row!["TOTAL", "", "", format_duration(&net_duration)]);
        table.printstd();

        // Display tasks if any exist.
        if !tasks.is_empty() {
            println!("\nTasks:");
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
    pub fn sum((daily_durations, total_duration, average_duration): &(HashMap<NaiveDate, String>, String, String)) -> Result<(), Box<dyn Error>> {
        let mut table: Table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["DATE", "DURATION"]);

        let mut dates: Vec<&NaiveDate> = daily_durations.keys().collect();
        dates.sort();
        for date in dates {
            if let Some(duration_str) = daily_durations.get(date) {
                table.add_row(row![date.format("%d.%m.%Y"), duration_str]);
            }
        }
        table.add_empty_row();
        table.add_row(row!["AVERAGE", average_duration]);
        table.add_row(row!["TOTAL", total_duration]);
        table.printstd();

        Ok(())
    }

    /// Displays a table of breaks for a given day.
    ///
    /// # Arguments
    /// * `breaks` - A slice of `Break` records to display.
    ///
    /// # Returns
    /// A `Result` indicating success.
    pub fn breaks(breaks: &[Break]) -> Result<(), Box<dyn Error>> {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "START", "END", "DURATION"]);
        for (i, b) in breaks.iter().enumerate() {
            table.add_row(row![
                i + 1,
                b.start.format("%H:%M"),
                b.end.map(|t| t.format("%H:%M").to_string()).unwrap_or_default(),
                b.duration.map(|duration: TimeDelta| format_duration(&duration)).unwrap_or_default()
            ]);
        }
        table.printstd();
        Ok(())
    }
}
