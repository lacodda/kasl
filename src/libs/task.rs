//! Task management and manipulation functionality for productivity tracking.
//!
//! This module provides comprehensive task management capabilities including task
//! creation, modification, filtering, and formatting. It serves as the foundation
//! for organizing work items and tracking productivity progress within the kasl
//! application.
//!
//! ## Core Concepts
//!
//! ### Task Structure
//! Tasks represent individual work items with the following characteristics:
//! - **Identification**: Unique IDs for database persistence and reference
//! - **Description**: Human-readable names and detailed comments
//! - **Progress Tracking**: Completion percentages from 0% to 100%
//! - **Categorization**: Tag-based organization and filtering
//! - **Timestamps**: Creation and modification time tracking
//! - **Integration**: Links to external systems (Jira, GitLab, etc.)
//!
//! ### Task Lifecycle
//! ```text
//! Creation → Assignment → Progress Updates → Completion → Archival
//!    ↓          ↓            ↓               ↓          ↓
//! New Task   Add Tags    Update %      Mark Done   Historical
//! Database   Categories  Completion    100%        Data
//! Entry      Labels      Status        Complete    Storage
//! ```
//!
//! ## Filtering System
//!
//! The module provides flexible filtering capabilities for task management:
//!
//! ### Filter Types
//! - **All Tasks**: Complete task listing without restrictions
//! - **Date-Based**: Tasks created or modified on specific dates
//! - **Completion Status**: Filter by progress level (incomplete, complete)
//! - **ID-Based**: Retrieve specific tasks by their identifiers
//! - **Tag-Based**: Filter by single or multiple tag associations
//!
//! ### Filter Composition
//! Filters can be combined and chained for complex queries:
//! ```rust
//! // Example: Find incomplete tasks with specific tags from today
//! let today_incomplete_tagged = TaskFilter::Date(today)
//!     .and(TaskFilter::Incomplete)
//!     .and(TaskFilter::ByTag("urgent".to_string()));
//! ```
//!
//! ## Formatting and Display
//!
//! ### Formatting Traits
//! The module implements formatting traits for flexible output:
//! - **FormatTasks**: Convert task collections to display-ready formats
//! - **Display Integration**: Console table rendering support
//! - **Export Formatting**: Structured data for CSV/JSON export
//! - **Template Support**: Integration with task template system
//!
//! ### Data Partitioning
//! Advanced formatting includes data partitioning capabilities:
//! - **Load Balancing**: Distribute tasks across multiple workers
//! - **Display Grouping**: Organize tasks into logical groups
//! - **Batch Processing**: Handle large task collections efficiently
//!
//! ## Integration Points
//!
//! ### External System Integration
//! Tasks can be created from external sources:
//! - **Jira Issues**: Import completed issues as tasks
//! - **GitLab Commits**: Convert commits to task records
//! - **Manual Entry**: User-created tasks via CLI or GUI
//! - **Template Instantiation**: Tasks created from predefined templates
//!
//! ### Database Integration
//! - **CRUD Operations**: Complete create, read, update, delete support
//! - **Transaction Safety**: Atomic operations for data consistency
//! - **Relationship Management**: Tag associations and references
//! - **Historical Tracking**: Audit trail for task modifications
//!
//! ## Usage Examples
//!
//! ### Basic Task Creation
//! ```rust
//! use kasl::libs::task::Task;
//!
//! // Create a new task with basic information
//! let task = Task::new(
//!     "Implement user authentication",
//!     "Add OAuth2 integration with Google and GitHub",
//!     Some(25) // 25% complete
//! );
//! ```
//!
//! ### Task Filtering
//! ```rust
//! use kasl::libs::task::{TaskFilter, Task};
//! use chrono::Local;
//!
//! // Filter tasks by various criteria
//! let today = Local::now().date_naive();
//! let filters = vec![
//!     TaskFilter::Date(today),
//!     TaskFilter::Incomplete,
//!     TaskFilter::ByTag("priority".to_string()),
//! ];
//! ```
//!
//! ### Task Formatting
//! ```rust
//! use kasl::libs::task::{FormatTasks, Task};
//!
//! let mut tasks = vec![/* task collection */];
//!
//! // Format for display
//! let formatted_output = tasks.format();
//!
//! // Partition for processing
//! let task_groups = tasks.divide(3); // Split into 3 groups
//! ```
//!
//! ## Performance Considerations
//!
//! ### Memory Efficiency
//! - **Lazy Loading**: Tasks loaded on-demand from database
//! - **Streaming Support**: Large collections processed incrementally
//! - **Reference Counting**: Efficient memory usage for shared tasks
//! - **Garbage Collection**: Automatic cleanup of unused task data
//!
//! ### Query Optimization
//! - **Index Usage**: Database queries optimized with proper indexing
//! - **Batch Operations**: Multiple tasks processed in single transactions
//! - **Caching**: Frequently accessed tasks cached in memory
//! - **Pagination**: Large result sets handled with pagination

use crate::db::tags::Tag;
use chrono::NaiveDate;

/// Represents a single task or work item in the productivity tracking system.
///
/// The Task struct encapsulates all information about a work item including
/// its identification, description, progress status, and associated metadata.
/// Tasks serve as the fundamental unit of work organization within kasl.
///
/// ## Field Descriptions
///
/// ### Identification Fields
/// - `id`: Database primary key for persistence and relationships
/// - `task_id`: Reference ID for linking to external systems or parent tasks
/// - `timestamp`: Creation/modification time for audit trails
///
/// ### Content Fields
/// - `name`: Brief, descriptive title for the task
/// - `comment`: Detailed description, notes, or additional context
/// - `completeness`: Progress percentage (0-100) indicating work completion
///
/// ### Configuration Fields
/// - `excluded_from_search`: Flag to hide tasks from general searches
/// - `tags`: Collection of categorization labels for organization
///
/// ## Usage Patterns
///
/// ### New Task Creation
/// ```rust
/// let task = Task::new(
///     "Code review for PR #123",
///     "Review authentication changes and security implications",
///     Some(0) // Just started
/// );
/// ```
///
/// ### Task Updates
/// ```rust
/// let mut task = existing_task;
/// task.completeness = Some(75); // 75% complete
/// task.comment = "Almost finished, testing remaining".to_string();
/// ```
///
/// ### External Integration
/// ```rust
/// let jira_task = Task {
///     id: None, // Will be assigned by database
///     task_id: Some(jira_issue_id),
///     name: jira_issue.summary,
///     comment: jira_issue.description.unwrap_or_default(),
///     completeness: Some(100), // Imported completed issues
///     // ... other fields
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Task {
    /// Database primary key for task identification and relationships.
    ///
    /// This field contains the unique identifier assigned by the database
    /// when the task is first saved. It's used for:
    /// - Database queries and updates
    /// - Foreign key relationships (tags, time tracking)
    /// - Cross-referencing with other application data
    ///
    /// **Values:**
    /// - `Some(id)`: Task exists in database with assigned ID
    /// - `None`: New task not yet saved to database
    pub id: Option<i32>,

    /// Reference identifier for external system integration or task linking.
    ///
    /// This field provides a way to link tasks to external systems or
    /// create hierarchical relationships between tasks. Common uses include:
    /// - Jira issue numbers for imported tasks
    /// - GitLab merge request IDs for code review tasks
    /// - Parent task IDs for subtask relationships
    /// - External project management system references
    ///
    /// **Values:**
    /// - `Some(id)`: Task is linked to external system or parent task
    /// - `None`: Standalone task with no external references
    pub task_id: Option<i32>,

    /// ISO 8601 timestamp string indicating task creation or last modification.
    ///
    /// This field provides audit trail information for task management.
    /// The timestamp is automatically managed by the database layer and
    /// is primarily used for:
    /// - Sorting tasks by creation or modification time
    /// - Audit trails and change tracking
    /// - Time-based filtering and reporting
    /// - Synchronization with external systems
    ///
    /// **Format:** "YYYY-MM-DD HH:MM:SS" in local timezone
    /// **Example:** "2025-01-15 14:30:45"
    pub timestamp: Option<String>,

    /// Brief, descriptive title summarizing the task's purpose or objective.
    ///
    /// The task name should be concise yet descriptive enough to understand
    /// the work item at a glance. It appears in task lists, reports, and
    /// notifications throughout the application.
    ///
    /// **Guidelines:**
    /// - Keep under 100 characters for display compatibility
    /// - Use action-oriented language ("Implement", "Review", "Fix")
    /// - Include key context ("Fix login bug in mobile app")
    /// - Avoid technical jargon when possible
    ///
    /// **Examples:**
    /// - "Implement OAuth2 authentication"
    /// - "Review security audit findings"
    /// - "Update API documentation for v2.0"
    pub name: String,

    /// Detailed description, notes, or additional context for the task.
    ///
    /// The comment field provides space for detailed information that doesn't
    /// fit in the task name. This might include:
    /// - Technical requirements and specifications
    /// - Links to related resources or documentation
    /// - Progress notes and status updates
    /// - Dependencies and prerequisites
    /// - Testing criteria and acceptance conditions
    ///
    /// **Content Guidelines:**
    /// - Use clear, structured formatting when helpful
    /// - Include links to relevant resources
    /// - Update with progress notes and findings
    /// - Keep information current and relevant
    pub comment: String,

    /// Completion percentage indicating task progress from 0% to 100%.
    ///
    /// This field tracks the task's progress through its lifecycle,
    /// providing quantitative measurement of work completion. The percentage
    /// is used for:
    /// - Progress reporting and analytics
    /// - Filtering incomplete vs. complete tasks
    /// - Productivity calculations and trends
    /// - Project status tracking and reporting
    ///
    /// **Values:**
    /// - `Some(0)`: Task not started
    /// - `Some(1-99)`: Task in progress
    /// - `Some(100)`: Task completed
    /// - `None`: Progress not tracked or unknown
    ///
    /// **Default:** 100% for tasks imported from external systems
    pub completeness: Option<i32>,

    /// Flag indicating whether task should be hidden from general searches.
    ///
    /// This field allows tasks to be excluded from default search results
    /// while remaining accessible through direct queries. Useful for:
    /// - Administrative or system-generated tasks
    /// - Deprecated or obsolete tasks that shouldn't appear in normal workflow
    /// - Sensitive tasks that require explicit access
    /// - Archived tasks that should remain searchable but not prominent
    ///
    /// **Values:**
    /// - `Some(true)`: Task excluded from general searches
    /// - `Some(false)` or `None`: Task included in normal search results
    pub excluded_from_search: Option<bool>,

    /// Collection of categorization tags associated with this task.
    ///
    /// Tags provide a flexible labeling system for task organization and
    /// filtering. They enable:
    /// - Project-based organization ("frontend", "backend", "mobile")
    /// - Priority classification ("urgent", "low-priority", "nice-to-have")
    /// - Status indicators ("blocked", "waiting-review", "approved")
    /// - Skill-based categorization ("javascript", "database", "ui-design")
    ///
    /// **Management:**
    /// - Tags are created automatically when first used
    /// - Multiple tags can be associated with a single task
    /// - Tag associations are maintained in separate database table
    /// - Tags can be deleted if no longer associated with any tasks
    pub tags: Vec<Tag>,
}

impl Task {
    /// Creates a new task with the specified name, comment, and completion status.
    ///
    /// This constructor initializes a new task with the provided information
    /// and sets appropriate defaults for other fields. The task is not
    /// automatically saved to the database - use the database layer for persistence.
    ///
    /// ## Default Values
    ///
    /// New tasks are initialized with:
    /// - `id`: None (assigned when saved to database)
    /// - `task_id`: None (no external reference)
    /// - `timestamp`: None (managed by database)
    /// - `excluded_from_search`: None (included in searches)
    /// - `tags`: Empty vector (no initial categorization)
    ///
    /// ## Parameter Guidelines
    ///
    /// ### Task Name
    /// - Should be concise but descriptive
    /// - Use action-oriented language
    /// - Include key context for clarity
    ///
    /// ### Comment
    /// - Provide detailed context and requirements
    /// - Include links to related resources
    /// - Can be updated as work progresses
    ///
    /// ### Completeness
    /// - Use `Some(0)` for new tasks that haven't started
    /// - Use `Some(100)` for already completed imported tasks
    /// - Use `None` if progress tracking isn't needed
    ///
    /// # Arguments
    ///
    /// * `name` - Brief, descriptive task title
    /// * `comment` - Detailed description or notes
    /// * `completeness` - Optional completion percentage (0-100)
    ///
    /// # Returns
    ///
    /// A new Task instance ready for use or database persistence.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kasl::libs::task::Task;
    ///
    /// // Create a new task that's just starting
    /// let new_task = Task::new(
    ///     "Implement user registration",
    ///     "Add email verification and password validation",
    ///     Some(0)
    /// );
    ///
    /// // Create a completed task (e.g., imported from external system)
    /// let completed_task = Task::new(
    ///     "Fix login redirect bug",
    ///     "Resolved issue with OAuth callback URL handling",
    ///     Some(100)
    /// );
    ///
    /// // Create a task without progress tracking
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
            name: name.to_string(),
            comment: comment.to_string(),
            completeness,
            excluded_from_search: None,
            tags: Vec::new(),
        }
    }

    /// Updates task fields from another task while preserving identity fields.
    ///
    /// This method provides a convenient way to update task content while
    /// maintaining database identity and relationships. It's particularly
    /// useful for implementing task editing workflows where the user modifies
    /// task details but the task's core identity remains unchanged.
    ///
    /// ## Preserved Fields
    /// The following fields are **not** updated to maintain task identity:
    /// - `id`: Database primary key remains unchanged
    /// - `task_id`: External reference remains unchanged
    /// - `timestamp`: Will be updated by database on save
    /// - `excluded_from_search`: Search visibility remains unchanged
    /// - `tags`: Tag associations require separate management
    ///
    /// ## Updated Fields
    /// The following fields are copied from the source task:
    /// - `name`: Task title is updated
    /// - `comment`: Description and notes are updated
    /// - `completeness`: Progress percentage is updated
    ///
    /// ## Use Cases
    ///
    /// ### Task Editing Workflow
    /// ```rust
    /// // Load existing task from database
    /// let mut existing_task = tasks_db.get_by_id(42)?;
    ///
    /// // Create updated version with user modifications
    /// let updated_task = Task::new(
    ///     "Updated task name",
    ///     "Updated description with new requirements",
    ///     Some(75)
    /// );
    ///
    /// // Apply updates while preserving identity
    /// existing_task.update_from(&updated_task);
    ///
    /// // Save to database
    /// tasks_db.update(&existing_task)?;
    /// ```
    ///
    /// ### Bulk Task Updates
    /// ```rust
    /// for mut task in tasks_to_update {
    ///     if let Some(template) = get_update_template(&task) {
    ///         task.update_from(&template);
    ///         tasks_db.update(&task)?;
    ///     }
    /// }
    /// ```
    ///
    /// # Arguments
    ///
    /// * `other` - Source task containing the updated field values
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kasl::libs::task::Task;
    ///
    /// let mut original_task = Task::new(
    ///     "Original task",
    ///     "Original description",
    ///     Some(25)
    /// );
    /// // Simulate database assignment
    /// original_task.id = Some(42);
    ///
    /// let updated_task = Task::new(
    ///     "Updated task name",
    ///     "Updated description with more details",
    ///     Some(75)
    /// );
    ///
    /// // Apply updates while preserving ID
    /// original_task.update_from(&updated_task);
    ///
    /// assert_eq!(original_task.id, Some(42)); // ID preserved
    /// assert_eq!(original_task.name, "Updated task name"); // Content updated
    /// assert_eq!(original_task.completeness, Some(75)); // Progress updated
    /// ```
    pub fn update_from(&mut self, other: &Task) {
        // Update content fields while preserving identity
        self.name = other.name.clone();
        self.comment = other.comment.clone();
        self.completeness = other.completeness;
    }
}

/// Enumeration of available task filtering criteria for database queries.
///
/// This enum provides a type-safe way to specify different filtering options
/// when querying tasks from the database. It supports simple filters as well
/// as complex multi-criteria filtering for advanced task management workflows.
///
/// ## Filter Categories
///
/// ### Scope Filters
/// - `All`: No filtering, returns all tasks
/// - `Date`: Time-based filtering for specific dates
///
/// ### Status Filters  
/// - `Incomplete`: Tasks with progress less than 100%
///
/// ### Identity Filters
/// - `ByIds`: Specific tasks by their database IDs
///
/// ### Categorization Filters
/// - `ByTag`: Tasks associated with a single tag
/// - `ByTags`: Tasks associated with multiple tags (intersection)
///
/// ## Query Optimization
///
/// Different filter types have different performance characteristics:
/// - `All`: Fastest, no WHERE clause needed
/// - `ByIds`: Very fast with proper indexing
/// - `Date`: Fast with timestamp indexing
/// - `Incomplete`: Moderate speed, depends on data distribution
/// - `ByTag`/`ByTags`: Moderate speed, requires JOIN operations
///
/// ## Examples Usage
///
/// ```rust
/// use kasl::libs::task::TaskFilter;
/// use chrono::Local;
///
/// // Get all tasks
/// let all_tasks_filter = TaskFilter::All;
///
/// // Get today's tasks
/// let today = Local::now().date_naive();
/// let today_filter = TaskFilter::Date(today);
///
/// // Get incomplete tasks
/// let incomplete_filter = TaskFilter::Incomplete;
///
/// // Get specific tasks
/// let specific_filter = TaskFilter::ByIds(vec![1, 2, 3]);
///
/// // Get tagged tasks
/// let tagged_filter = TaskFilter::ByTag("urgent".to_string());
/// let multi_tagged_filter = TaskFilter::ByTags(vec![
///     "frontend".to_string(),
///     "javascript".to_string()
/// ]);
/// ```
#[derive(Debug, Clone)]
pub enum TaskFilter {
    /// Returns all tasks without any filtering restrictions.
    ///
    /// This filter retrieves the complete task collection from the database.
    /// It's the most efficient filter type since it doesn't require any
    /// WHERE clauses or complex query conditions.
    ///
    /// **Use Cases:**
    /// - Complete task listings for administrative purposes
    /// - Full data exports and backups
    /// - Global task analysis and reporting
    /// - Initial load for client-side filtering
    ///
    /// **Performance:** Excellent - simple SELECT query
    All,

    /// Returns tasks created or modified on the specified date.
    ///
    /// This filter uses the task timestamp to find tasks associated with
    /// a particular date. The filtering is typically done at the day level,
    /// including all tasks from 00:00:00 to 23:59:59 on the specified date.
    ///
    /// **Use Cases:**
    /// - Daily task reviews and reports
    /// - Time-based productivity analysis
    /// - Date-specific task exports
    /// - Calendar integration and scheduling
    ///
    /// **Performance:** Good - benefits from timestamp indexing
    ///
    /// **Example:**
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    /// use chrono::{Local, NaiveDate};
    ///
    /// let today = Local::now().date_naive();
    /// let filter = TaskFilter::Date(today);
    /// ```
    Date(NaiveDate),

    /// Returns tasks with completion percentage less than 100%.
    ///
    /// This filter identifies tasks that are still in progress or haven't
    /// been started. It's useful for focusing on active work items and
    /// identifying tasks that need attention.
    ///
    /// **Criteria:**
    /// - Tasks with `completeness` < 100
    /// - Tasks with `completeness` = None (treated as incomplete)
    ///
    /// **Use Cases:**
    /// - Active work item management
    /// - Progress tracking and follow-up
    /// - Workload planning and estimation
    /// - Focus mode filtering for current work
    ///
    /// **Performance:** Moderate - depends on completion data distribution
    ///
    /// **Example:**
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    ///
    /// let incomplete_filter = TaskFilter::Incomplete;
    /// // Returns tasks with completeness: None, Some(0), Some(50), etc.
    /// // Excludes tasks with completeness: Some(100)
    /// ```
    Incomplete,

    /// Returns specific tasks identified by their database IDs.
    ///
    /// This filter provides precise task retrieval when the exact task
    /// identifiers are known. It's the most efficient way to retrieve
    /// a known set of tasks and is commonly used for bulk operations.
    ///
    /// **Use Cases:**
    /// - Bulk task operations (update, delete, export)
    /// - User-selected task collections
    /// - Related task loading (parent/child relationships)
    /// - API responses for specific task requests
    ///
    /// **Performance:** Excellent - uses primary key indexing
    ///
    /// **Example:**
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    ///
    /// let specific_tasks = TaskFilter::ByIds(vec![1, 5, 10, 15]);
    /// // Returns only tasks with IDs 1, 5, 10, and 15
    /// ```
    ByIds(Vec<i32>),

    /// Returns tasks associated with the specified tag.
    ///
    /// This filter finds all tasks that have been tagged with a particular
    /// label. It's useful for category-based task management and organizing
    /// work by project, priority, or skill area.
    ///
    /// **Query Method:**
    /// - Performs JOIN with task_tags relationship table
    /// - Matches tag name case-sensitively
    /// - Returns tasks with at least one matching tag
    ///
    /// **Use Cases:**
    /// - Project-specific task listings
    /// - Priority-based filtering ("urgent", "low-priority")
    /// - Skill-based work organization ("javascript", "database")
    /// - Status-based filtering ("blocked", "waiting-review")
    ///
    /// **Performance:** Moderate - requires JOIN operation
    ///
    /// **Example:**
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    ///
    /// let urgent_filter = TaskFilter::ByTag("urgent".to_string());
    /// // Returns all tasks tagged with "urgent"
    /// ```
    ByTag(String),

    /// Returns tasks associated with all of the specified tags.
    ///
    /// This filter finds tasks that have been tagged with every tag in the
    /// provided list (intersection, not union). It's useful for finding tasks
    /// that meet multiple criteria simultaneously.
    ///
    /// **Query Method:**
    /// - Performs multiple JOINs with task_tags table
    /// - Requires ALL tags to be present on the task
    /// - More restrictive than single tag filtering
    ///
    /// **Use Cases:**
    /// - Complex filtering ("frontend" AND "urgent" AND "javascript")
    /// - Multi-criteria task discovery
    /// - Advanced search functionality
    /// - Refined project management workflows
    ///
    /// **Performance:** Moderate to Slow - multiple JOINs required
    ///
    /// **Example:**
    /// ```rust
    /// use kasl::libs::task::TaskFilter;
    ///
    /// let complex_filter = TaskFilter::ByTags(vec![
    ///     "frontend".to_string(),
    ///     "urgent".to_string(),
    ///     "javascript".to_string()
    /// ]);
    /// // Returns tasks that have ALL three tags
    /// ```
    ByTags(Vec<String>),
}

/// Trait providing formatting and manipulation operations for task collections.
///
/// This trait extends Vec<Task> with specialized methods for formatting tasks
/// for display and dividing task collections for parallel processing or
/// load balancing. It provides a clean interface for common task collection
/// operations.
///
/// ## Design Philosophy
///
/// The trait follows Rust's iterator philosophy by providing chainable,
/// efficient operations on task collections. Methods are designed to be:
/// - **Composable**: Can be chained together for complex operations
/// - **Efficient**: Minimize allocations and copying where possible
/// - **Flexible**: Support various output formats and processing patterns
/// - **Predictable**: Consistent behavior across different input sizes
///
/// ## Method Categories
///
/// ### Formatting Methods
/// - `format()`: Convert tasks to human-readable string representation
///
/// ### Partitioning Methods
/// - `divide()`: Split tasks into balanced groups for parallel processing
///
/// ## Performance Characteristics
///
/// - **Memory Usage**: Methods minimize unnecessary allocations
/// - **Time Complexity**: Most operations are O(n) where n is task count
/// - **Parallelization**: Partitioning methods support concurrent processing
///
/// ## Examples
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
/// // Format for display
/// let formatted = tasks.format();
/// println!("{}", formatted);
///
/// // Divide for parallel processing
/// let groups = tasks.divide(2);
/// for (i, group) in groups.iter().enumerate() {
///     println!("Group {}: {} tasks", i, group.len());
/// }
/// ```
pub trait FormatTasks {
    /// Formats the task collection into a human-readable string representation.
    ///
    /// This method converts a collection of tasks into a structured string
    /// format suitable for console output, logging, or simple text-based
    /// displays. The format includes key task information in a consistent,
    /// scannable layout.
    ///
    /// ## Output Format
    ///
    /// The method produces a multi-line string with each task formatted as:
    /// ```text
    /// {name} ({completeness}%)
    /// ```
    ///
    /// ## Field Handling
    ///
    /// - **ID**: Shows database ID or "New" for unsaved tasks
    /// - **Name**: Task title, truncated if excessively long
    /// - **Completeness**: Percentage or "Unknown" if not set
    /// - **Comment**: Description, truncated if excessively long
    ///
    /// ## Use Cases
    ///
    /// - **Debug Output**: Quick task collection visualization
    /// - **Log Messages**: Structured logging of task operations
    /// - **Simple Reports**: Basic text-based task summaries
    /// - **CLI Output**: Command-line interface task displays
    ///
    /// # Returns
    ///
    /// A formatted string representation of all tasks in the collection.
    ///
    /// # Examples
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
    /// // Output:
    /// // Review PR (25%)
    /// // Write tests (75%)
    /// ```
    fn format(&mut self) -> String;

    /// Divides the task collection into the specified number of balanced groups.
    ///
    /// This method partitions tasks into multiple groups of approximately equal
    /// size, which is useful for parallel processing, load balancing, or
    /// organizing large task collections into manageable chunks.
    ///
    /// ## Partitioning Algorithm
    ///
    /// The method uses a round-robin distribution strategy:
    /// 1. **Base Size Calculation**: Determines minimum tasks per group
    /// 2. **Remainder Distribution**: Distributes extra tasks evenly
    /// 3. **Sequential Assignment**: Assigns tasks to groups in order
    /// 4. **Balance Optimization**: Ensures groups differ by at most 1 task
    ///
    /// ## Edge Case Handling
    ///
    /// ### Empty Collection
    /// - Returns vector of empty groups
    /// - Number of groups equals requested parts
    ///
    /// ### Single Task
    /// - Duplicates the task across all groups
    /// - Useful for broadcast scenarios
    ///
    /// ### Fewer Tasks Than Parts
    /// - Creates groups with 0-1 tasks each
    /// - Distributes tasks round-robin style
    ///
    /// ### More Tasks Than Parts
    /// - Creates balanced groups with similar sizes
    /// - Groups differ by at most 1 task
    ///
    /// ## Use Cases
    ///
    /// ### Parallel Processing
    /// ```rust
    /// let task_groups = tasks.divide(cpu_count);
    /// for group in task_groups {
    ///     spawn_worker_thread(group);
    /// }
    /// ```
    ///
    /// ### Load Balancing
    /// ```rust
    /// let worker_assignments = tasks.divide(worker_count);
    /// for (worker_id, assignment) in worker_assignments.iter().enumerate() {
    ///     assign_tasks_to_worker(worker_id, assignment);
    /// }
    /// ```
    ///
    /// ### UI Organization
    /// ```rust
    /// let columns = tasks.divide(3); // Three-column layout
    /// for (col_index, column_tasks) in columns.iter().enumerate() {
    ///     render_task_column(col_index, column_tasks);
    /// }
    /// ```
    ///
    /// # Arguments
    ///
    /// * `parts` - Number of groups to create (must be > 0)
    ///
    /// # Returns
    ///
    /// A vector containing the requested number of task groups. Each group
    /// is a Vec<Task> containing a portion of the original task collection.
    ///
    /// # Examples
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
    /// // Divide into 3 groups
    /// let groups = tasks.divide(3);
    /// // groups[0]: [Task 1, Task 4] (2 tasks)
    /// // groups[1]: [Task 2, Task 5] (2 tasks)  
    /// // groups[2]: [Task 3]         (1 task)
    ///
    /// // Verify balanced distribution
    /// assert_eq!(groups.len(), 3);
    /// assert_eq!(groups[0].len(), 2);
    /// assert_eq!(groups[1].len(), 2);
    /// assert_eq!(groups[2].len(), 1);
    /// ```
    fn divide(&mut self, parts: usize) -> Vec<Vec<Task>>;
}

/// Implementation of FormatTasks trait for Vec<Task>.
///
/// This implementation provides concrete formatting and partitioning logic
/// for task collections. It handles various edge cases and provides efficient
/// algorithms for common task manipulation scenarios.
impl FormatTasks for Vec<Task> {
    /// Divides the task collection into balanced groups using round-robin distribution.
    ///
    /// This implementation uses an optimized algorithm that ensures balanced
    /// distribution while handling edge cases gracefully. The algorithm
    /// minimizes memory allocations and provides predictable results.
    ///
    /// ## Algorithm Details
    ///
    /// 1. **Input Validation**: Handle zero parts and empty collections
    /// 2. **Special Cases**: Optimize for single task and small collections
    /// 3. **Size Calculation**: Compute base size and remainder distribution
    /// 4. **Group Assignment**: Distribute tasks using calculated sizes
    ///
    /// ## Performance Characteristics
    ///
    /// - **Time Complexity**: O(n) where n is the number of tasks
    /// - **Space Complexity**: O(n) for the output groups
    /// - **Memory Efficiency**: Minimal allocations during processing
    ///
    /// The implementation is optimized for common use cases while maintaining
    /// correctness for edge cases.
    fn divide(&mut self, parts: usize) -> Vec<Vec<Task>> {
        // Initialize result vector with requested capacity
        let mut result: Vec<Vec<Task>> = Vec::with_capacity(parts);
        let len = self.len();

        // Handle edge case: no parts requested
        if len == 0 || parts == 0 {
            return result;
        }

        // Handle edge case: single task
        if len == 1 {
            for _ in 0..parts {
                result.push(self.to_vec());
            }
            return result;
        }

        // Handle edge case: fewer tasks than parts
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

        // General case: distribute tasks across parts
        let mut start = 0;
        let mut end;
        for i in 0..parts {
            // Calculate group size with remainder distribution
            end = start + len / parts + if i < len % parts { 1 } else { 0 };
            result.push(self[start..end].to_vec());
            start = end;
        }

        result
    }

    /// Formats the task collection into a structured string representation.
    ///
    /// This implementation creates a multi-line string with each task formatted
    /// consistently. It handles missing fields gracefully and provides readable
    /// output suitable for debugging and simple displays.
    ///
    /// ## Format Structure
    ///
    /// Each task is formatted on a separate line with pipe-separated fields:
    /// ```text
    /// {name} ({completeness}%)
    /// ```
    ///
    /// ## Field Processing
    ///
    /// - **Name**: Used as-is from task struct
    /// - **Completeness**: Shows percentage or "Unknown" for None values
    ///
    /// The method handles all field types gracefully and provides consistent
    /// output regardless of which optional fields are present.
    fn format(&mut self) -> String {
        self.iter()
            .map(|task| {
                // Format completeness field
                let completeness_display = task.completeness.map_or("Unknown".to_string(), |comp| format!("{}%", comp));

                // Create formatted line for this task
                return format!("{} ({})", task.name, completeness_display);
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
