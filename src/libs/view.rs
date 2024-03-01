use super::{event::FormatEvent, task::Task};
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

    pub fn events((events, total_duration): &(Vec<FormatEvent>, String)) -> Result<(), Box<dyn Error>> {
        let mut table: Table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(row!["ID", "START", "END", "DURATION"]);

        for event in events.iter() {
            table.add_row(row![event.id, event.start, event.end, event.duration]);
        }
        table.add_empty_row();
        table.add_row(row!["TOTAL", "", "", total_duration]);
        table.printstd();

        Ok(())
    }
}
