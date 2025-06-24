use super::task::Task;
use crate::db::workdays::Workday;
use crate::libs::r#break::Break;
use chrono::{Duration, NaiveDate};
use prettytable::{format, row, Table};
use std::collections::HashMap;
use std::error::Error;

/// Manages the display of data in a formatted console output.
pub struct View {}

impl View {
    /// Displays a table of tasks with their details.
    ///
    /// Formats tasks into a table with columns for ID, Task ID, Name, Comment, and Completeness.
    ///
    /// # Arguments
    /// * `tasks` - A vector of tasks to display.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if table rendering fails.
    pub fn tasks(tasks: &Vec<Task>) -> Result<(), Box<dyn Error>> {
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

    /// Displays a work report with intervals and tasks for a given day.
    ///
    /// Generates a table of work intervals (START, END, DURATION) based on workday start/end times
    /// and breaks, followed by a total net work time. If tasks exist, they are displayed in a separate table.
    ///
    /// # Arguments
    /// * `workday` - The workday record containing start and optional end times.
    /// * `breaks` - A vector of breaks for the day.
    /// * `tasks` - A vector of tasks for the day.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if table rendering fails.
    pub fn report(workday: &Workday, breaks: &Vec<Break>, tasks: &Vec<Task>) -> Result<(), Box<dyn Error>> {
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
        table.set_titles(row!["START", "END", "DURATION"]);

        for (start, end, duration) in intervals {
            table.add_row(row![start.format("%H:%M"), end.format("%H:%M"), format_duration(duration)]);
        }
        table.add_empty_row();
        table.add_row(row!["TOTAL", "", format_duration(net_duration)]);
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
    /// Formats a table with daily durations, total duration, and average duration.
    ///
    /// # Arguments
    /// * `daily_durations` - A map of dates to formatted duration strings.
    /// * `total_duration` - The total duration as a formatted string.
    /// * `average_duration` - The average duration as a formatted string.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if table rendering fails.
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
    /// Formats breaks with their ID, start time, end time, and duration.
    ///
    /// # Arguments
    /// * `breaks` - A vector of breaks to display.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if table rendering fails.
    pub fn breaks(breaks: &Vec<Break>) -> Result<(), Box<dyn Error>> {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "START", "END", "DURATION"]);
        for (i, b) in breaks.iter().enumerate() {
            table.add_row(row![
                i + 1,
                b.start.format("%H:%M"),
                b.end.map(|t| t.format("%H:%M").to_string()).unwrap_or_default(),
                b.duration.map(format_duration).unwrap_or_default()
            ]);
        }
        table.printstd();
        Ok(())
    }
}

/// Formats a duration into a "HH:MM" string.
///
/// # Arguments
/// * `duration` - The duration to format.
///
/// # Returns
/// A string in the format "HH:MM" representing hours and minutes.
fn format_duration(duration: Duration) -> String {
    let hours = duration.num_hours();
    let mins = duration.num_minutes() % 60;
    format!("{:02}:{:02}", hours, mins)
}
