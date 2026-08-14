//! The task model, its query filters, and name normalization.
//!
//! ```rust
//! use kasl::libs::task::Task;
//!
//! let task = Task::new(
//!     "Implement user authentication",
//!     "Add OAuth2 integration with Google and GitHub",
//!     Some(25)
//! );
//! ```

use crate::db::tags::Tag;
use chrono::NaiveDate;

/// A single work item.
///
/// `task_id` links to an external system (a Jira issue id, a GitLab MR);
/// `timestamp` is managed by the database layer.
///
/// ```rust
/// use kasl::libs::task::Task;
///
/// let task = Task::new(
///     "Code review for PR #123",
///     "Review authentication changes and security implications",
///     Some(0) // Just started
/// );
/// ```
///
/// ```rust
/// use kasl::libs::task::Task;
///
/// let existing_task = Task::new("Existing task", "Details", Some(50));
/// let mut task = existing_task;
/// task.completeness = Some(75);
/// task.comment = "Almost finished, testing remaining".to_string();
/// ```
///
/// Populating a task from an external issue:
/// ```rust
/// use kasl::libs::task::Task;
///
/// // Simulated Jira issue data used to populate a task.
/// let jira_issue_id = 42;
/// struct JiraIssue {
///     summary: String,
///     description: Option<String>,
/// }
/// let jira_issue = JiraIssue {
///     summary: "Fix login bug".to_string(),
///     description: Some("Users cannot log in with SSO".to_string()),
/// };
///
/// let jira_task = Task {
///     id: None, // Will be assigned by database
///     task_id: Some(jira_issue_id),
///     timestamp: None,
///     name: jira_issue.summary,
///     comment: jira_issue.description.unwrap_or_default(),
///     completeness: Some(100), // Imported completed issues
///     excluded_from_search: None,
///     tags: vec![],
/// };
/// # let _ = jira_task;
/// ```
#[derive(Debug, Clone)]
pub struct Task {
    /// Database primary key; `None` until the task is saved.
    pub id: Option<i32>,

    /// External reference (Jira issue id, GitLab MR id); `None` for standalone tasks.
    pub task_id: Option<i32>,

    /// `"YYYY-MM-DD HH:MM:SS"` in local time, set by the database layer.
    pub timestamp: Option<String>,

    /// Task title.
    pub name: String,

    /// Free-form notes.
    pub comment: String,

    /// Progress 0-100; imported completed issues default to 100.
    pub completeness: Option<i32>,

    /// Hidden from task discovery when true.
    pub excluded_from_search: Option<bool>,

    /// Tags, maintained through the `task_tags` relationship table.
    pub tags: Vec<Tag>,
}

impl Task {
    /// Creates an unsaved task; whitespace in `name` and `comment` is collapsed.
    ///
    /// ```rust
    /// use kasl::libs::task::Task;
    ///
    /// let new_task = Task::new(
    ///     "Implement user registration",
    ///     "Add email verification and password validation",
    ///     Some(0)
    /// );
    ///
    /// let completed_task = Task::new(
    ///     "Fix login redirect bug",
    ///     "Resolved issue with OAuth callback URL handling",
    ///     Some(100)
    /// );
    ///
    /// let planning_task = Task::new(
    ///     "Research authentication libraries",
    ///     "Evaluate OAuth2 libraries for Node.js backend",
    ///     None
    /// );
    /// ```
    pub fn new(name: &str, comment: &str, completeness: Option<i32>) -> Self {
        Task {
            id: None,
            task_id: None,
            timestamp: None,
            name: collapse_whitespace(name),
            comment: collapse_whitespace(comment),
            completeness,
            excluded_from_search: None,
            tags: Vec::new(),
        }
    }

    /// Copies `name`, `comment` and `completeness` from `other`, keeping
    /// identity fields (`id`, `task_id`, `timestamp`, search flag, tags).
    ///
    /// ```rust,no_run
    /// # fn f() -> anyhow::Result<()> {
    /// use kasl::libs::task::Task;
    /// use kasl::db::tasks::Tasks;
    ///
    /// let mut tasks_db = Tasks::new()?;
    /// let mut existing_task = tasks_db.get_by_id(42)?.expect("task exists");
    ///
    /// let updated_task = Task::new(
    ///     "Updated task name",
    ///     "Updated description with new requirements",
    ///     Some(75)
    /// );
    ///
    /// existing_task.update_from(&updated_task);
    /// tasks_db.update(&existing_task)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```rust,no_run
    /// # fn f() -> anyhow::Result<()> {
    /// use kasl::libs::task::Task;
    /// use kasl::db::tasks::Tasks;
    ///
    /// let mut tasks_db = Tasks::new()?;
    /// let tasks_to_update: Vec<Task> = vec![];
    /// let get_update_template = |_task: &Task| -> Option<Task> { None };
    ///
    /// for mut task in tasks_to_update {
    ///     if let Some(template) = get_update_template(&task) {
    ///         task.update_from(&template);
    ///         tasks_db.update(&task)?;
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```rust
    /// use kasl::libs::task::Task;
    ///
    /// let mut original_task = Task::new(
    ///     "Original task",
    ///     "Original description",
    ///     Some(25)
    /// );
    /// original_task.id = Some(42);
    ///
    /// let updated_task = Task::new(
    ///     "Updated task name",
    ///     "Updated description with more details",
    ///     Some(75)
    /// );
    ///
    /// original_task.update_from(&updated_task);
    ///
    /// assert_eq!(original_task.id, Some(42)); // ID preserved
    /// assert_eq!(original_task.name, "Updated task name"); // Content updated
    /// assert_eq!(original_task.completeness, Some(75)); // Progress updated
    /// ```
    pub fn update_from(&mut self, other: &Task) {
        self.name = other.name.clone();
        self.comment = other.comment.clone();
        self.completeness = other.completeness;
    }
}

/// Filtering criteria for task queries.
///
/// ```rust
/// use kasl::libs::task::TaskFilter;
/// use chrono::Local;
///
/// let all_tasks_filter = TaskFilter::All;
///
/// let today = Local::now().date_naive();
/// let today_filter = TaskFilter::Date(today);
///
/// let incomplete_filter = TaskFilter::Incomplete;
/// let specific_filter = TaskFilter::ByIds(vec![1, 2, 3]);
///
/// let tagged_filter = TaskFilter::ByTag("urgent".to_string());
/// let multi_tagged_filter = TaskFilter::ByTags(vec![
///     "frontend".to_string(),
///     "javascript".to_string()
/// ]);
/// ```
#[derive(Debug, Clone)]
pub enum TaskFilter {
    /// Every task, no filtering.
    All,

    /// Tasks whose timestamp falls on the given local date.
    ///
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    /// use chrono::{Local, NaiveDate};
    ///
    /// let today = Local::now().date_naive();
    /// let filter = TaskFilter::Date(today);
    /// ```
    Date(NaiveDate),

    /// Tasks below 100% done; `completeness: None` counts as incomplete.
    ///
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    ///
    /// let incomplete_filter = TaskFilter::Incomplete;
    /// // Returns tasks with completeness: None, Some(0), Some(50), etc.
    /// // Excludes tasks with completeness: Some(100)
    /// ```
    Incomplete,

    /// Tasks with the given database ids.
    ///
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    ///
    /// let specific_tasks = TaskFilter::ByIds(vec![1, 5, 10, 15]);
    /// ```
    ByIds(Vec<i32>),

    /// Tasks carrying the tag (name matched case-sensitively).
    ///
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    ///
    /// let urgent_filter = TaskFilter::ByTag("urgent".to_string());
    /// ```
    ByTag(String),

    /// Tasks carrying ALL of the tags - intersection, not union.
    ///
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    ///
    /// let complex_filter = TaskFilter::ByTags(vec![
    ///     "frontend".to_string(),
    ///     "urgent".to_string(),
    ///     "javascript".to_string()
    /// ]);
    /// ```
    ByTags(Vec<String>),
}

/// Display formatting and partitioning for task collections.
///
/// ```rust
/// use kasl::libs::task::{Task, FormatTasks};
///
/// let mut tasks = vec![
///     Task::new("Task 1", "Description 1", Some(50)),
///     Task::new("Task 2", "Description 2", Some(75)),
///     Task::new("Task 3", "Description 3", Some(100)),
/// ];
///
/// let formatted = tasks.format();
/// println!("{}", formatted);
///
/// let groups = tasks.divide(2);
/// for (i, group) in groups.iter().enumerate() {
///     println!("Group {}: {} tasks", i, group.len());
/// }
/// ```
pub trait FormatTasks {
    /// Renders one `{name} ({completeness}%)` line per task.
    ///
    /// ```rust
    /// use kasl::libs::task::{Task, FormatTasks};
    ///
    /// let mut tasks = vec![
    ///     Task::new("Review PR", "Code review for auth changes", Some(25)),
    ///     Task::new("Write tests", "Unit tests for API endpoints", Some(75)),
    /// ];
    ///
    /// let output = tasks.format();
    /// // Review PR (25%)
    /// // Write tests (75%)
    /// ```
    fn format(&mut self) -> String;

    /// Splits the collection into `parts` groups differing by at most one
    /// task. A single task is duplicated into every group; fewer tasks than
    /// parts distributes round-robin.
    ///
    /// ```rust
    /// use kasl::libs::task::{Task, FormatTasks};
    ///
    /// let mut tasks = vec![
    ///     Task::new("Task 1", "", None),
    ///     Task::new("Task 2", "", None),
    ///     Task::new("Task 3", "", None),
    ///     Task::new("Task 4", "", None),
    ///     Task::new("Task 5", "", None),
    /// ];
    ///
    /// let groups = tasks.divide(3);
    ///
    /// assert_eq!(groups.len(), 3);
    /// assert_eq!(groups[0].len(), 2);
    /// assert_eq!(groups[1].len(), 2);
    /// assert_eq!(groups[2].len(), 1);
    /// ```
    fn divide(&mut self, parts: usize) -> Vec<Vec<Task>>;
}

impl FormatTasks for Vec<Task> {
    fn divide(&mut self, parts: usize) -> Vec<Vec<Task>> {
        let mut result: Vec<Vec<Task>> = Vec::with_capacity(parts);
        let len = self.len();

        if len == 0 || parts == 0 {
            return result;
        }

        // A single task is broadcast to every group.
        if len == 1 {
            for _ in 0..parts {
                result.push(self.to_vec());
            }
            return result;
        }

        // Fewer tasks than parts: round-robin so every group gets something.
        if len < parts {
            for i in 0..parts {
                let mut part: Vec<Task> = Vec::with_capacity(len.div_ceil(parts));
                for j in 0..len.div_ceil(parts) {
                    part.push(self[(i + j * len / parts) % len].clone());
                }
                result.push(part);
            }
            return result;
        }

        // General case: contiguous slices, remainder spread over the first groups.
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
            .map(|task| {
                let completeness_display = task.completeness.map_or("Unknown".to_string(), |comp| format!("{}%", comp));
                format!("{} ({})", task.name, completeness_display)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Replaces newlines and other whitespace runs with single spaces and trims.
///
/// Useful when pasting multi-line titles into `kasl task` prompts:
/// ```text
/// PROJ-42
/// Fix login redirect
/// ```
/// becomes `PROJ-42 Fix login redirect`.
pub fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalizes a task/commit name for near-duplicate and ignore-list comparison.
///
/// Trims whitespace, collapses internal spaces, lowercases, and strips
/// trailing punctuation so variants like `"New commit"`, `"New commit."`,
/// and `" New commit"` map to the same key.
pub fn normalize_task_name(name: &str) -> String {
    let mut s = collapse_whitespace(name).to_lowercase();

    loop {
        let trimmed = s.trim_end_matches(['.', ',', ';', '!', '?', ':', '…']).trim_end();
        if trimmed.len() == s.len() {
            break;
        }
        s = trimmed.to_string();
    }

    s
}

/// Returns true when `name` matches an ignore pattern exactly or by prefix
/// (after normalization). Used for task discovery filtering.
pub fn is_ignored_name(name: &str, ignore_names: &[String]) -> bool {
    let n = normalize_task_name(name);
    ignore_names.iter().any(|pat| {
        let p = normalize_task_name(pat);
        !p.is_empty() && (n == p || n.starts_with(&p))
    })
}
