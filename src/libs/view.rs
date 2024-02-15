use super::task::Task;
use prettytable::{row, Table};
use std::error::Error;

pub struct View {}

impl View {
    pub fn tasks(tasks: &Vec<Task>) -> Result<(), Box<dyn Error>> {
        let mut table = Table::new();

        table.add_row(row!["ID", "TASK ID", "NAME", "COMMENT", "COMPLETENESS"]);
        for task in tasks {
            table.add_row(row![
                task.id.unwrap_or(0),
                task.task_id.unwrap_or(0),
                task.name,
                task.comment,
                task.completeness.unwrap_or(100)
            ]);
        }
        table.printstd();

        Ok(())
    }
}
