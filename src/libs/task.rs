#[derive(Debug)]
pub struct Task {
    pub id: Option<i32>,
    pub timestamp: Option<String>,
    pub name: String,
    pub comment: String,
    pub completeness: i32,
}

impl Task {
    pub fn new(name: &str, comment: &str, completeness: &i32) -> Self {
        Task {
            id: None,
            timestamp: None,
            name: name.to_string(),
            comment: comment.to_string(),
            completeness: completeness.to_owned(),
        }
    }
}
