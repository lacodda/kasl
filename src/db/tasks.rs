//! Comprehensive task management database operations and querying.
//!
//! This module provides the core functionality for managing tasks within the kasl
//! application. It handles all database interactions for task creation, modification,
//! deletion, and retrieval with support for advanced filtering, tagging, and
//! relationship management.
//!
//! ## Task Management Features
//!
//! - **CRUD Operations**: Complete Create, Read, Update, Delete functionality
//! - **Advanced Filtering**: Multi-criteria task querying with date, completion, and tag filters  
//! - **Batch Operations**: Efficient bulk deletion and modification operations
//! - **Tag Integration**: Seamless integration with the tagging system for categorization
//! - **Task Hierarchies**: Support for parent-child task relationships
//! - **Search Capabilities**: Flexible task discovery and filtering mechanisms
//!
//! ## Database Schema
//!
//! The `tasks` table structure:
//! - `id`: Primary key for unique task identification
//! - `task_id`: Reference to parent task for hierarchical relationships
//! - `timestamp`: Creation time with automatic local timezone handling
//! - `name`: Task title/description (required, human-readable)
//! - `comment`: Optional detailed description or notes
//! - `completeness`: Progress percentage (0-100, defaults to 100)
//! - `excluded_from_search`: Flag to hide tasks from certain queries
//!
//! ## Task Lifecycle
//!
//! 1. **Creation**: Tasks are created with automatic timestamp and ID assignment
//! 2. **Association**: Tags can be linked to provide categorization
//! 3. **Updates**: Properties can be modified while preserving relationships
//! 4. **Completion**: Progress tracking through completion percentage
//! 5. **Archival**: Soft deletion and search exclusion capabilities
//!
//! ## Filtering and Querying
//!
//! The module supports sophisticated task filtering:
//! - **Date-based**: Retrieve tasks created on specific dates
//! - **Completion-based**: Find incomplete tasks from recent periods
//! - **Tag-based**: Filter by single or multiple tag associations
//! - **ID-based**: Direct lookup by task identifiers
//! - **Comprehensive**: Retrieve all tasks without restrictions
//!
//! ## Usage Examples
//!
//! ```rust
//! use kasl::db::tasks::{Tasks, TaskFilter};
//! use kasl::libs::task::Task;
//! use chrono::Local;
//!
//! let mut tasks = Tasks::new()?;
//!
//! // Create a new task
//! let task = Task::new("Review code", "Check PR #123", Some(75));
//! tasks.insert(&task)?;
//!
//! // Fetch tasks for today
//! let today_tasks = tasks.fetch(TaskFilter::Date(Local::now().date_naive()))?;
//!
//! // Find incomplete tasks
//! let incomplete = tasks.fetch(TaskFilter::Incomplete)?;
//!
//! // Update task progress
//! let mut task = tasks.get_by_id(task_id)?.unwrap();
//! task.completeness = 100;
//! tasks.update(&task)?;
//! ```

use super::db::Db;
use crate::libs::messages::Message;
use crate::libs::task::{Task, TaskFilter};
use crate::msg_error_anyhow;
use anyhow::Result;
use rusqlite::{params, Connection, Statement, ToSql};
use std::vec;

/// SQL schema definition for the tasks table.
///
/// Defines the complete structure for storing task information with support for:
/// - Unique identification and hierarchical relationships
/// - Temporal tracking with automatic timestamp generation
/// - Progress monitoring through completion percentages
/// - Flexible content storage with optional descriptions
/// - Search and visibility control mechanisms
const SCHEMA_TASKS: &str = "CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER NOT NULL PRIMARY KEY,
    task_id INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 0,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 100,
    excluded_from_search BOOLEAN NOT NULL ON CONFLICT REPLACE DEFAULT FALSE
);";

/// Insert a new task with full field specification and return the assigned ID.
///
/// Creates a task record with automatic timestamp generation and immediate
/// ID return for further operations. Supports all task properties including
/// hierarchical relationships and search visibility controls.
const INSERT_TASK: &str = "INSERT INTO tasks (task_id, timestamp, name, comment, completeness, excluded_from_search) VALUES
    (?, datetime(CURRENT_TIMESTAMP, 'localtime'), ?, ?, ?, ?) RETURNING id";

/// Update the parent task relationship for an existing task.
///
/// Establishes or modifies hierarchical task relationships by setting
/// the task_id field to reference another task as the parent.
const UPDATE_TASK_ID: &str = "UPDATE tasks SET task_id = ? WHERE id = ?";

/// Base query for selecting all task fields.
///
/// Foundation query that retrieves complete task records with all
/// properties. Used as the base for more specific filtering queries.
const SELECT_TASKS: &str = "SELECT * FROM tasks";

/// Date-based filtering clause for tasks created on a specific date.
///
/// Filters tasks by creation date using local timezone conversion
/// to ensure accurate date matching across different time zones.
const WHERE_DATE: &str = "WHERE date(timestamp) = date(?1, 'localtime')";

/// ID-based filtering clause for retrieving specific tasks.
///
/// Dynamically constructed clause for filtering tasks by a list
/// of specific task_id values using IN operator.
const WHERE_ID_IN: &str = "WHERE task_id IN";

/// Complex filtering for incomplete tasks from recent periods.
///
/// Sophisticated query that finds tasks meeting multiple criteria:
/// - Completion status less than 100%
/// - Not already present in today's task list
/// - Represents the latest completion state from the past 15 days
/// - Groups by task_id to avoid duplicates
const WHERE_INCOMPLETE: &str = "WHERE
  completeness < 100 AND
  task_id NOT IN (SELECT task_id FROM tasks WHERE DATE(timestamp) = DATE('now')) AND
  (task_id, completeness) IN (SELECT task_id, MAX(completeness) FROM tasks
  WHERE DATE(timestamp) BETWEEN datetime(CURRENT_TIMESTAMP, 'localtime', '-15 day') AND datetime(CURRENT_TIMESTAMP, 'localtime', '-1 day')
  GROUP BY task_id)
  GROUP BY task_id";

/// Tag-based filtering for tasks associated with a specific tag.
///
/// Joins tasks with the tag system to filter by tag name,
/// enabling categorical task retrieval and organization.
const WHERE_TAG: &str = "WHERE id IN (SELECT task_id FROM task_tags tt JOIN tags t ON tt.tag_id = t.id WHERE t.name = ?1)";

/// Multiple tag filtering for tasks associated with any of the specified tags.
///
/// Extends single tag filtering to support multiple tag names,
/// allowing for more flexible task categorization and retrieval.
const WHERE_TAGS: &str = "WHERE id IN (SELECT task_id FROM task_tags tt JOIN tags t ON tt.tag_id = t.id WHERE t.name IN";

/// Delete a single task record by its unique identifier.
///
/// Permanently removes a task from the database. Note that this will
/// also cascade to remove task-tag relationships due to foreign key constraints.
const DELETE_TASK: &str = "DELETE FROM tasks WHERE id = ?";

/// Bulk deletion query for removing multiple tasks efficiently.
///
/// Dynamically constructed query for deleting multiple tasks in a single
/// database operation, improving performance over individual deletions.
const DELETE_TASKS_BY_IDS: &str = "DELETE FROM tasks WHERE id IN";

/// Existence check query for validating task presence.
///
/// Efficiently determines if a task with the specified ID exists
/// without retrieving the full task data.
const SELECT_COUNT_BY_ID: &str = "SELECT COUNT(*) FROM tasks WHERE id = ?";

/// Update query for modifying existing task properties.
///
/// Updates the core task properties (name, comment, completeness)
/// while preserving the task ID, timestamp, and relationships.
const UPDATE_TASK: &str = "UPDATE tasks SET name = ?, comment = ?, completeness = ? WHERE id = ?";

/// Database interface for comprehensive task management operations.
///
/// The `Tasks` struct provides a high-level API for all task-related database
/// operations, managing connections, transactions, and result processing.
/// It maintains state for the most recently inserted task ID to support
/// method chaining and immediate post-creation operations.
///
/// ## Architecture
///
/// - **Connection Management**: Direct SQLite connection for optimal performance
/// - **State Tracking**: Maintains last insertion ID for operation chaining
/// - **Error Handling**: Comprehensive error propagation and message generation
/// - **Transaction Support**: Implicit transaction handling for data consistency
///
/// ## Performance Considerations
///
/// - Prepared statements are used for frequently executed queries
/// - Bulk operations are optimized for large dataset manipulation
/// - Index-aware query construction for efficient filtering
/// - Tag integration is lazy-loaded to minimize overhead
#[derive(Debug)]
pub struct Tasks {
    /// Direct SQLite database connection for task operations.
    ///
    /// Provides transactional access to the tasks table and related
    /// structures. Connection is managed internally and provides
    /// optimal performance for task-specific operations.
    pub conn: Connection,

    /// Identifier of the most recently inserted task.
    ///
    /// Maintained automatically by insertion operations to support
    /// method chaining and immediate post-creation tasks like
    /// tag association or hierarchical relationship establishment.
    pub id: Option<i32>,
}

impl Tasks {
    /// Creates a new Tasks manager and initializes the database schema.
    ///
    /// This constructor establishes a database connection, ensures the tasks
    /// table schema is properly initialized, and prepares the manager for
    /// task operations. Schema creation is idempotent and safe for repeated calls.
    ///
    /// # Returns
    ///
    /// Returns a new `Tasks` instance ready for task management operations,
    /// or an error if database initialization fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::tasks::Tasks;
    ///
    /// let mut tasks = Tasks::new()?;
    /// // Ready for task operations
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database connection cannot be established
    /// - Schema creation fails due to permissions or corruption
    /// - Migration system encounters errors during table setup
    pub fn new() -> Result<Self> {
        let db = Db::new()?;

        // Initialize the tasks table schema
        db.conn.execute(&SCHEMA_TASKS, [])?;

        Ok(Self { conn: db.conn, id: None })
    }

    /// Inserts a new task into the database and returns a mutable reference for chaining.
    ///
    /// Creates a new task record with automatic ID assignment and timestamp generation.
    /// The task's completion status, parent relationships, and search visibility are
    /// all properly configured during insertion. The assigned ID is stored internally
    /// for subsequent operations.
    ///
    /// ## Automatic Field Handling
    ///
    /// - **ID Assignment**: Database automatically assigns unique primary key
    /// - **Timestamp**: Current local time is set as creation timestamp
    /// - **Validation**: Required fields are validated before insertion
    /// - **Defaults**: Missing optional fields receive appropriate default values
    ///
    /// # Arguments
    ///
    /// * `task` - Task object containing the properties to insert
    ///
    /// # Returns
    ///
    /// Returns a mutable reference to self for method chaining, allowing
    /// immediate follow-up operations like tag association or updates.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::tasks::Tasks;
    /// use kasl::libs::task::Task;
    ///
    /// let mut tasks = Tasks::new()?;
    /// let task = Task::new("Code review", "Review PR #123", Some(50));
    /// tasks.insert(&task)?
    ///      .update_id()?; // Method chaining
    /// ```
    ///
    /// # Database Effects
    ///
    /// - Creates new record in tasks table
    /// - Triggers any associated database triggers
    /// - Updates internal state with new task ID
    /// - Maintains referential integrity with tag system
    pub fn insert(&mut self, task: &Task) -> Result<&mut Self> {
        // Execute insertion query and capture the returned ID
        self.id = Some(self.conn.query_row(
            INSERT_TASK,
            params![task.task_id, task.name, task.comment, task.completeness, task.excluded_from_search],
            |row| row.get(0),
        )?);

        Ok(self)
    }

    /// Updates the parent task relationship for the most recently inserted task.
    ///
    /// This method sets the task_id field to establish hierarchical relationships
    /// between tasks. It operates on the task ID stored from the most recent
    /// insertion, making it ideal for immediate post-creation relationship setup.
    ///
    /// ## Hierarchical Task Support
    ///
    /// - **Parent-Child Relationships**: Link tasks in hierarchical structures
    /// - **Project Organization**: Group related tasks under parent tasks
    /// - **Dependency Tracking**: Establish task dependencies and workflows
    /// - **Nested Task Management**: Support for multi-level task hierarchies
    ///
    /// # Returns
    ///
    /// Returns a mutable reference to self for continued method chaining,
    /// or an error if no recent insertion ID is available.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tasks = Tasks::new()?;
    /// let subtask = Task::new("Subtask", "Part of larger task", Some(0));
    /// tasks.insert(&subtask)?
    ///      .update_id()?; // Set task_id to reference itself or parent
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No recent insertion ID is available (call after insert)
    /// - Database update operation fails
    /// - Referential integrity constraints are violated
    pub fn update_id(&mut self) -> Result<&mut Self> {
        self.conn.execute(UPDATE_TASK_ID, params![self.id, self.id])?;
        Ok(self)
    }

    /// Retrieves the most recently inserted task as a vector.
    ///
    /// This convenience method fetches the complete task record for the
    /// most recently inserted task, including all associated tags and
    /// relationships. Useful for immediate verification of insertion results.
    ///
    /// # Returns
    ///
    /// Returns a vector containing the single most recent task, or an error
    /// if no recent insertion ID is available or the task cannot be found.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tasks = Tasks::new()?;
    /// let task = Task::new("New task", "Description", Some(100));
    /// let inserted_tasks = tasks.insert(&task)?
    ///                          .get()?; // Retrieve the inserted task
    /// ```
    ///
    /// # Error Conditions
    ///
    /// - No recent insertion ID available
    /// - Task was deleted after insertion
    /// - Database access failures
    pub fn get(&mut self) -> Result<Vec<Task>> {
        let id = self.id.ok_or_else(|| msg_error_anyhow!(Message::NoIdSet))?;
        self.fetch(TaskFilter::ByIds(vec![id]))
    }

    /// Fetches tasks based on sophisticated filtering criteria.
    ///
    /// This is the primary query method that supports all task filtering scenarios
    /// through a unified interface. It dynamically constructs SQL queries based on
    /// the provided filter, executes them efficiently, and enriches results with
    /// associated tag information.
    ///
    /// ## Supported Filters
    ///
    /// - **All**: Retrieves every task in the database without restrictions
    /// - **Date**: Tasks created on a specific date (local timezone)
    /// - **Incomplete**: Tasks with progress < 100% from recent periods
    /// - **ByIds**: Direct lookup of tasks by their unique identifiers
    /// - **ByTag**: Tasks associated with a specific tag name
    /// - **ByTags**: Tasks associated with any of multiple tag names
    ///
    /// ## Query Optimization
    ///
    /// The method uses prepared statements and parameterized queries for security
    /// and performance. Complex filters like "Incomplete" use sophisticated SQL
    /// to efficiently identify the latest completion status for each task.
    ///
    /// ## Tag Integration
    ///
    /// All returned tasks are automatically enriched with their associated tag
    /// information through a secondary query. This provides complete task context
    /// without requiring separate tag lookups.
    ///
    /// # Arguments
    ///
    /// * `filter` - The filtering criteria to apply when retrieving tasks
    ///
    /// # Returns
    ///
    /// Returns a vector of tasks matching the filter criteria, with complete
    /// tag information included. Empty vector if no tasks match the filter.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::tasks::{Tasks, TaskFilter};
    /// use chrono::Local;
    ///
    /// let mut tasks = Tasks::new()?;
    ///
    /// // Get all tasks
    /// let all_tasks = tasks.fetch(TaskFilter::All)?;
    ///
    /// // Get today's tasks
    /// let today = tasks.fetch(TaskFilter::Date(Local::now().date_naive()))?;
    ///
    /// // Get tasks tagged as "urgent"
    /// let urgent = tasks.fetch(TaskFilter::ByTag("urgent".to_string()))?;
    ///
    /// // Get incomplete tasks
    /// let incomplete = tasks.fetch(TaskFilter::Incomplete)?;
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Uses prepared statements for optimal query performance
    /// - Complex filters like Incomplete may be slower on large datasets
    /// - Tag information is loaded separately and may add overhead
    /// - Results are loaded entirely into memory
    pub fn fetch(&mut self, filter: TaskFilter) -> Result<Vec<Task>> {
        // Construct the appropriate query and parameters based on filter type
        let (mut stmt, params): (Statement, Vec<Box<dyn ToSql>>) = match filter {
            TaskFilter::All => (self.conn.prepare(SELECT_TASKS)?, vec![]),
            TaskFilter::Date(date) => (self.conn.prepare(&format!("{} {}", SELECT_TASKS, WHERE_DATE))?, vec![Box::new(date)]),
            TaskFilter::Incomplete => (self.conn.prepare(&format!("{} {}", SELECT_TASKS, WHERE_INCOMPLETE))?, vec![]),
            TaskFilter::ByIds(ids) => {
                let ids_params: Vec<Box<dyn ToSql>> = ids.clone().into_iter().map(|id| Box::new(id) as Box<dyn ToSql>).collect();
                (self.conn.prepare(&Self::query_by_ids(&ids))?, ids_params)
            }
            TaskFilter::ByTag(tag_name) => (self.conn.prepare(&format!("{} {}", SELECT_TASKS, WHERE_TAG))?, vec![Box::new(tag_name)]),
            TaskFilter::ByTags(tag_names) => {
                let placeholders = vec!["?"; tag_names.len()].join(", ");
                let query = format!("{} {} ({}))", SELECT_TASKS, WHERE_TAGS, placeholders);
                let params: Vec<Box<dyn ToSql>> = tag_names.into_iter().map(|name| Box::new(name) as Box<dyn ToSql>).collect();
                (self.conn.prepare(&query)?, params)
            }
        };

        // Execute the query with proper parameter binding
        let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| &**p).collect();
        let task_iter = stmt.query_map(&params_refs[..], |row| {
            Ok(Task {
                id: row.get(0)?,
                task_id: row.get(1)?,
                timestamp: row.get(2)?,
                name: row.get(3)?,
                comment: row.get(4)?,
                completeness: row.get(5)?,
                excluded_from_search: row.get(6)?,
                tags: vec![], // Tags will be populated in the next step
            })
        })?;

        // Collect all task results
        let mut tasks = Vec::new();
        for task_result in task_iter {
            tasks.push(task_result?);
        }

        // Enrich tasks with tag information
        let mut tags_db = crate::db::tags::Tags::new()?;
        for task in &mut tasks {
            if let Some(task_id) = task.id {
                task.tags = tags_db.get_tags_by_task(task_id)?;
            }
        }

        Ok(tasks)
    }

    /// Constructs a dynamic SQL query for ID-based task filtering.
    ///
    /// This helper method generates the appropriate WHERE clause for filtering
    /// tasks by a list of specific IDs. It dynamically creates the correct number
    /// of parameter placeholders to match the provided ID list.
    ///
    /// # Arguments
    ///
    /// * `ids` - Vector of task IDs to include in the query
    ///
    /// # Returns
    ///
    /// Returns a properly formatted SQL query string with parameter placeholders.
    ///
    /// # Example Output
    ///
    /// For `ids = vec![1, 2, 3]`:
    /// ```sql
    /// SELECT * FROM tasks WHERE task_id IN (?, ?, ?)
    /// ```
    fn query_by_ids(ids: &Vec<i32>) -> String {
        format!("{} {} ({})", SELECT_TASKS, WHERE_ID_IN, vec!["?"; ids.len()].join(", "))
    }

    /// Deletes a single task by its unique identifier.
    ///
    /// Permanently removes a task record from the database along with all
    /// associated relationships such as tag assignments. The operation is
    /// atomic and cannot be undone without database backups.
    ///
    /// ## Cascade Effects
    ///
    /// Due to foreign key constraints, deleting a task will also:
    /// - Remove all task-tag associations
    /// - Update any child tasks that reference this task as parent
    /// - Trigger any associated database cleanup procedures
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier of the task to delete
    ///
    /// # Returns
    ///
    /// Returns the number of records affected (0 or 1), or an error if
    /// the database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tasks = Tasks::new()?;
    /// let deleted_count = tasks.delete(task_id)?;
    /// if deleted_count > 0 {
    ///     println!("Task deleted successfully");
    /// }
    /// ```
    ///
    /// # Safety Considerations
    ///
    /// - Deletion is immediate and permanent
    /// - No confirmation prompts at this level
    /// - Callers should implement appropriate confirmation flows
    /// - Consider soft deletion for recoverable scenarios
    pub fn delete(&mut self, id: i32) -> Result<usize> {
        let affected = self.conn.execute(DELETE_TASK, params![id])?;
        Ok(affected)
    }

    /// Efficiently deletes multiple tasks in a single database transaction.
    ///
    /// Performs bulk deletion of tasks specified by their IDs, providing
    /// better performance than individual delete operations and ensuring
    /// atomicity. All specified tasks and their relationships are removed
    /// in a single transaction.
    ///
    /// ## Performance Benefits
    ///
    /// - Single SQL statement execution reduces database round trips
    /// - Transaction-based operation ensures consistency
    /// - More efficient than individual deletion loops
    /// - Reduced lock contention on high-concurrency scenarios
    ///
    /// ## Transaction Behavior
    ///
    /// The operation is atomic - either all specified tasks are deleted
    /// or none are deleted if any error occurs. This prevents partial
    /// deletion states that could lead to data inconsistency.
    ///
    /// # Arguments
    ///
    /// * `ids` - Slice of task IDs to delete
    ///
    /// # Returns
    ///
    /// Returns the total number of tasks actually deleted, or an error
    /// if the database operation fails. Count may be less than input
    /// length if some IDs don't exist.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tasks = Tasks::new()?;
    /// let ids_to_delete = vec![101, 102, 103];
    /// let deleted_count = tasks.delete_many(&ids_to_delete)?;
    /// println!("Deleted {} tasks", deleted_count);
    /// ```
    ///
    /// # Edge Cases
    ///
    /// - Empty input slice returns 0 without database interaction
    /// - Non-existent IDs are silently ignored in the count
    /// - Very large ID lists may hit database query limits
    pub fn delete_many(&mut self, ids: &[i32]) -> Result<usize> {
        // Handle empty input to avoid unnecessary database operations
        if ids.is_empty() {
            return Ok(0);
        }

        // Construct dynamic query with appropriate number of placeholders
        let placeholders = vec!["?"; ids.len()].join(", ");
        let query = format!("{} ({})", DELETE_TASKS_BY_IDS, placeholders);

        // Convert IDs to boxed ToSql trait objects for parameter binding
        let params: Vec<Box<dyn ToSql>> = ids.iter().map(|id| Box::new(*id) as Box<dyn ToSql>).collect();
        let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| &**p).collect();

        // Execute the bulk deletion query
        let affected = self.conn.execute(&query, &params_refs[..])?;
        Ok(affected)
    }

    /// Checks if a task with the specified ID exists in the database.
    ///
    /// Efficiently determines task existence without retrieving the full
    /// task data. This method is useful for validation before performing
    /// operations that require existing tasks.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier of the task to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the task exists, `false` otherwise, or an error
    /// if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tasks = Tasks::new()?;
    /// if tasks.exists(task_id)? {
    ///     println!("Task exists and can be updated");
    /// } else {
    ///     println!("Task not found");
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// This method uses COUNT(*) which is optimized for existence checking
    /// and is more efficient than retrieving full task records when only
    /// existence verification is needed.
    pub fn exists(&mut self, id: i32) -> Result<bool> {
        let count: i32 = self.conn.query_row(SELECT_COUNT_BY_ID, params![id], |row| row.get(0))?;
        Ok(count > 0)
    }

    /// Updates an existing task's properties in the database.
    ///
    /// Modifies the core properties of an existing task (name, comment, completion)
    /// while preserving the task's ID, timestamp, and relationships. The task must
    /// have a valid ID from a previous database operation.
    ///
    /// ## Update Scope
    ///
    /// This method updates the following fields:
    /// - **name**: Task title/description
    /// - **comment**: Detailed description or notes
    /// - **completeness**: Progress percentage (0-100)
    ///
    /// The following fields are **not** modified:
    /// - **id**: Primary key remains unchanged
    /// - **timestamp**: Creation time is preserved
    /// - **task_id**: Parent relationships require separate operations
    /// - **excluded_from_search**: Search visibility requires separate handling
    ///
    /// # Arguments
    ///
    /// * `task` - Task object with updated properties and valid ID
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the update succeeds, or an error if the operation
    /// fails or the task doesn't exist.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tasks = Tasks::new()?;
    /// let mut task = tasks.get_by_id(task_id)?.unwrap();
    /// task.name = "Updated task name".to_string();
    /// task.completeness = 75;
    /// tasks.update(&task)?;
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Task doesn't have a valid ID (not saved to database)
    /// - No task exists with the specified ID
    /// - Database constraints are violated
    /// - Connection or transaction failures occur
    pub fn update(&mut self, task: &Task) -> Result<()> {
        // Ensure task has a valid database ID
        let id = task.id.ok_or_else(|| msg_error_anyhow!(Message::NoIdSet))?;

        // Execute update query and check if any rows were affected
        let affected = self.conn.execute(UPDATE_TASK, params![task.name, task.comment, task.completeness, id])?;

        // Verify that the task actually existed and was updated
        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TaskUpdateFailed));
        }

        Ok(())
    }

    /// Retrieves a single task by its unique identifier.
    ///
    /// This convenience method fetches a complete task record including
    /// all associated tags and properties. It's a specialized version of
    /// the fetch method optimized for single-task retrieval.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier of the task to retrieve
    ///
    /// # Returns
    ///
    /// Returns `Some(Task)` if found, `None` if the task doesn't exist,
    /// or an error if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tasks = Tasks::new()?;
    /// if let Some(task) = tasks.get_by_id(42)? {
    ///     println!("Found task: {}", task.name);
    /// } else {
    ///     println!("Task with ID 42 not found");
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// This method internally uses the fetch mechanism with ID filtering,
    /// so it includes full tag loading and relationship resolution.
    /// For simple existence checking, use the `exists` method instead.
    pub fn get_by_id(&mut self, id: i32) -> Result<Option<Task>> {
        let mut tasks = self.fetch(TaskFilter::ByIds(vec![id]))?;
        Ok(tasks.pop())
    }
}
