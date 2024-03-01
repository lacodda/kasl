use super::{event::Event, task::Task};
use chrono::Duration;
use prettytable::{format, row, Table};
use std::error::Error;

pub struct View {}

impl View {
    pub fn tasks(tasks: &Vec<Task>) -> Result<(), Box<dyn Error>> {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "TASK ID", "NAME", "COMMENT", "COMPLETENESS"]);

        for (index, task) in tasks.iter().enumerate() {
            table.add_row(row![index + 1, task.task_id.unwrap_or(0), task.name, task.comment, task.completeness.unwrap_or(100)]);
        }
        table.printstd();

        Ok(())
    }

    pub fn events((events, total_duration): &(Vec<Event>, Duration)) -> Result<(), Box<dyn Error>> {
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "START", "END", "DURATION"]);

        for (index, event) in events.iter().enumerate() {
            table.add_row(row![
                index + 1,
                event.start.format("%H:%M"),
                event.end.unwrap().format("%H:%M"),
                Self::format_duration(event.duration)
            ]);
        }
        table.add_empty_row();
        table.add_row(row!["TOTAL", "", "", Self::format_duration(Some(*total_duration))]);
        table.printstd();

        Ok(())
    }

    fn format_duration(duration_opt: Option<Duration>) -> String {
        duration_opt.map_or_else(
            || "--:--".to_string(),
            |duration| {
                let hours = duration.num_hours();
                let mins = duration.num_minutes() % 60;
                format!("{:02}:{:02}", hours, mins)
            },
        )
    }
}
