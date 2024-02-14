#[derive(Debug)]
pub struct Task {
    pub id: Option<i32>,
    pub task_id: Option<i32>,
    pub timestamp: Option<String>,
    pub name: String,
    pub comment: String,
    pub completeness: Option<i32>,
    pub excluded_from_search: Option<bool>,
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
        }
    }
}

#[derive(Debug, Clone)]
pub enum TaskFilter {
    All,
    Today,
    ByIds(Vec<i32>),
}
