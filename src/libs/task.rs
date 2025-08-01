use crate::db::tags::Tag;
use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct Task {
    pub id: Option<i32>,
    pub task_id: Option<i32>,
    pub timestamp: Option<String>,
    pub name: String,
    pub comment: String,
    pub completeness: Option<i32>,
    pub excluded_from_search: Option<bool>,
    pub tags: Vec<Tag>,
}

impl Task {
    pub fn new(name: &str, comment: &str, completeness: Option<i32>) -> Self {
        Task {
            id: None,
            task_id: None,
            timestamp: None,
            name: name.to_string(),
            comment: comment.to_string(),
            completeness,
            excluded_from_search: None,
            tags: Vec::new(),
        }
    }

    /// Update task fields from another task, preserving ID and task_id
    pub fn update_from(&mut self, other: &Task) {
        self.name = other.name.clone();
        self.comment = other.comment.clone();
        self.completeness = other.completeness;
    }
}

#[derive(Debug, Clone)]
pub enum TaskFilter {
    All,
    Date(NaiveDate),
    Incomplete,
    ByIds(Vec<i32>),
    ByTag(String),
    ByTags(Vec<String>),
}

pub trait FormatTasks {
    fn format(&mut self) -> String;
    fn divide(&mut self, parts: usize) -> Vec<Vec<Task>>;
}

impl FormatTasks for Vec<Task> {
    fn divide(&mut self, parts: usize) -> Vec<Vec<Task>> {
        let mut result: Vec<Vec<Task>> = Vec::with_capacity(parts);
        let len = self.len();

        if len == 0 || parts == 0 {
            return result;
        }

        if len == 1 {
            for _ in 0..parts {
                result.push(self.to_vec());
            }
            return result;
        }

        if len < parts {
            for i in 0..parts {
                let mut part: Vec<Task> = Vec::with_capacity((len + parts - 1) / parts);
                for j in 0..(len + parts - 1) / parts {
                    part.push(self[(i + j * len / parts) % len].clone());
                }
                result.push(part);
            }
            return result;
        }

        let mut start = 0;
        let mut end;
        for i in 0..parts {
            end = start + len / parts + if i < len % parts { 1 } else { 0 };
            result.push(self[start..end].to_vec());
            start = end;
        }

        result
    }

    fn format(&mut self) -> String {
        self.iter()
            .map(|task| format!("{} ({}%)", task.name, task.completeness.map_or(String::from("?"), |c| c.to_string())))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
