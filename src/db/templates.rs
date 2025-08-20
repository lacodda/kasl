//! Task template system for efficient task creation and workflow standardization.
//!
//! Provides functionality for managing reusable task templates that streamline
//! the creation of frequently used tasks. Templates store predefined task
//! configurations including names, descriptions, and completion status.
//!
//! ## Features
//!
//! - **Template Management**: Create, update, delete, and query template definitions
//! - **Search Capabilities**: Find templates by name or task content with fuzzy matching
//! - **Workflow Standardization**: Consistent task creation for repetitive workflows
//! - **Content Reuse**: Store commonly used task patterns for rapid deployment
//! - **Validation Support**: Ensure template uniqueness and data integrity
//!
//! ## Usage
//!
//! ```rust
//! use kasl::db::templates::{Templates, TaskTemplate};
//!
//! let mut templates = Templates::new()?;
//! let template = TaskTemplate::new(
//!     "daily-standup".to_string(),
//!     "Prepare for daily standup".to_string(),
//!     "Review yesterday's work and plan today".to_string(),
//!     50
//! );
//! templates.create(&template)?;
//! ```

use crate::db::db::Db;
use crate::libs::messages::Message;
use crate::msg_error_anyhow;
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// SQL schema for the task templates table.
///
/// Defines the structure for storing reusable task templates with unique
/// naming, content specifications, and automatic timestamp tracking.
/// The schema supports efficient lookups and ensures template uniqueness.
const SCHEMA_TEMPLATES: &str = "CREATE TABLE IF NOT EXISTS task_templates (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    task_name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER DEFAULT 100,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";

/// Insert a new template with complete field specification.
///
/// Creates a template record with automatic ID assignment and timestamp
/// generation. Template names must be unique across the system.
const INSERT_TEMPLATE: &str = "INSERT INTO task_templates (name, task_name, comment, completeness) VALUES (?1, ?2, ?3, ?4)";

/// Update an existing template's content while preserving metadata.
///
/// Modifies template properties (task_name, comment, completeness) while
/// maintaining the unique template name and preserving creation timestamp.
const UPDATE_TEMPLATE: &str = "UPDATE task_templates SET task_name = ?2, comment = ?3, completeness = ?4 WHERE name = ?1";

/// Delete a template record by its unique name identifier.
///
/// Permanently removes a template definition from the system. Template
/// deletion does not affect tasks that were previously created from the template.
const DELETE_TEMPLATE: &str = "DELETE FROM task_templates WHERE name = ?1";

/// Retrieve all templates ordered alphabetically by template name.
///
/// Provides a complete list of available templates sorted for consistent
/// display in user interfaces and selection dialogs.
const SELECT_ALL_TEMPLATES: &str = "SELECT * FROM task_templates ORDER BY name";

/// Find a specific template by its unique name identifier.
///
/// Enables direct template lookup for validation, editing, and task
/// creation operations using the human-readable template name.
const SELECT_TEMPLATE_BY_NAME: &str = "SELECT * FROM task_templates WHERE name = ?1";

/// Search templates by name or task content with fuzzy matching.
///
/// Supports partial matching across both template names and task names
/// to help users discover relevant templates quickly. Uses SQL LIKE
/// operator for flexible pattern matching.
const SEARCH_TEMPLATES: &str = "SELECT * FROM task_templates WHERE name LIKE ?1 OR task_name LIKE ?1 ORDER BY name";

/// Represents a reusable task template with predefined values.
///
/// A task template serves as a blueprint for creating tasks with consistent
/// properties. Templates encapsulate common task patterns and provide
/// default values that can be used directly or modified during task creation.
///
/// ## Design Philosophy
///
/// Templates separate the template identity (name) from the actual task
/// content (task_name), allowing for readable template names while
/// maintaining flexible task naming. This enables templates like
/// "daily-standup" to create tasks named "Prepare for daily standup meeting".
///
/// ## Field Relationships
///
/// - **name**: Identifies the template itself (e.g., "code-review-template")
/// - **task_name**: The actual task title when created (e.g., "Review PR #123")
/// - **comment**: Default description for context and instructions
/// - **completeness**: Default progress state (useful for different task types)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    /// Database-assigned unique identifier.
    ///
    /// Automatically set when the template is saved to the database.
    /// Used for internal references and database operations.
    pub id: Option<i32>,

    /// Unique template identifier for human-readable reference.
    ///
    /// This is the name users will use to identify and select templates.
    /// Must be unique across all templates and should be descriptive
    /// of the template's purpose (e.g., "daily-standup", "code-review").
    pub name: String,

    /// The actual task name that will be used when creating tasks.
    ///
    /// This becomes the task title when a task is created from this template.
    /// Can contain placeholders or generic descriptions that users can
    /// customize during task creation.
    pub task_name: String,

    /// Default description or notes for tasks created from this template.
    ///
    /// Provides context, instructions, or checklist items that help
    /// users understand what the task involves. Can include formatting
    /// and detailed guidance for task execution.
    pub comment: String,

    /// Default completion percentage when tasks are created from this template.
    ///
    /// Allows templates to specify different starting completion states:
    /// - 0: For tasks that start completely unfinished
    /// - 50: For tasks that are partially pre-completed
    /// - 100: For tasks that are considered complete when created (e.g., automated tasks)
    pub completeness: i32,

    /// Timestamp when the template was created.
    ///
    /// Automatically managed by the database for audit trails and
    /// chronological sorting. Used for template management and history.
    pub created_at: Option<String>,
}

impl TaskTemplate {
    /// Creates a new task template with the specified properties.
    ///
    /// This constructor creates a template object ready for database insertion.
    /// The ID and creation timestamp are automatically assigned when the
    /// template is saved using the `Templates::create()` method.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique identifier for the template (user-facing name)
    /// * `task_name` - Default task title for tasks created from this template
    /// * `comment` - Default description/instructions for template-based tasks
    /// * `completeness` - Default completion percentage (0-100)
    ///
    /// # Returns
    ///
    /// Returns a new `TaskTemplate` instance ready for database operations.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::templates::TaskTemplate;
    ///
    /// let template = TaskTemplate::new(
    ///     "morning-routine".to_string(),
    ///     "Complete morning routine".to_string(),
    ///     "Check emails, review calendar, plan day".to_string(),
    ///     25 // Start at 25% since some prep is already done
    /// );
    /// ```
    ///
    /// # Design Considerations
    ///
    /// - Template names should be URL-safe and easy to type
    /// - Task names can be more descriptive and user-friendly
    /// - Comments should provide actionable guidance
    /// - Completion values should reflect realistic starting states
    pub fn new(name: String, task_name: String, comment: String, completeness: i32) -> Self {
        Self {
            id: None,
            name,
            task_name,
            comment,
            completeness,
            created_at: None,
        }
    }
}

/// Database manager for task template operations and lifecycle management.
///
/// The `Templates` struct provides a comprehensive interface for managing
/// task templates, including creation, modification, deletion, and discovery
/// operations. It handles database connections and ensures data integrity
/// for all template-related operations.
///
/// ## Functionality Overview
///
/// - **CRUD Operations**: Complete Create, Read, Update, Delete support
/// - **Search and Discovery**: Flexible template finding and filtering
/// - **Validation**: Ensures template uniqueness and data consistency
/// - **Batch Operations**: Efficient handling of multiple template operations
///
/// ## Database Integration
///
/// The struct automatically manages database schema initialization and
/// provides transaction support for complex operations. It's designed
/// to work seamlessly with the broader kasl database ecosystem.
pub struct Templates {
    /// Direct database connection for template operations.
    ///
    /// Provides optimized access to the task_templates table with
    /// proper transaction support and connection management.
    conn: Connection,
}

impl Templates {
    /// Creates a new Templates manager and initializes the database schema.
    ///
    /// This constructor establishes a database connection, ensures the
    /// task_templates table exists with the proper schema, and prepares
    /// the manager for template operations. Schema creation is idempotent
    /// and integrates with the migration system.
    ///
    /// # Returns
    ///
    /// Returns a new `Templates` instance ready for template management,
    /// or an error if database initialization fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::templates::Templates;
    ///
    /// let mut templates = Templates::new()?;
    /// // Ready for template operations
    /// ```
    ///
    /// # Database Integration
    ///
    /// The templates table is officially created by migration v2, but this
    /// method ensures the table exists even in non-standard initialization
    /// scenarios. This provides robustness across different deployment patterns.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database connection cannot be established
    /// - Schema creation fails due to permissions or corruption
    /// - Migration system encounters errors during initialization
    pub fn new() -> Result<Self> {
        let db = Db::new()?;

        // Ensure the templates table exists (migration v2 creates it officially)
        db.conn.execute(SCHEMA_TEMPLATES, [])?;

        Ok(Templates { conn: db.conn })
    }

    /// Creates a new template in the database and validates uniqueness.
    ///
    /// This method inserts a new template record with the provided properties,
    /// automatically assigning a unique ID and creation timestamp. Template
    /// names must be unique across the entire system.
    ///
    /// ## Validation Process
    ///
    /// - **Uniqueness Check**: Ensures template name doesn't already exist
    /// - **Data Validation**: Validates required fields and constraints
    /// - **Integrity Enforcement**: Maintains database consistency rules
    /// - **Automatic Fields**: Sets ID and timestamp automatically
    ///
    /// # Arguments
    ///
    /// * `template` - Template object containing the properties to store
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the template is created successfully, or an error
    /// if creation fails due to uniqueness violations or database issues.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::templates::{Templates, TaskTemplate};
    ///
    /// let mut templates = Templates::new()?;
    /// let template = TaskTemplate::new(
    ///     "weekly-review".to_string(),
    ///     "Weekly review and planning".to_string(),
    ///     "Review accomplishments and plan next week".to_string(),
    ///     0
    /// );
    /// templates.create(&template)?;
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A template with the same name already exists
    /// - Required fields are missing or invalid
    /// - Database constraints are violated
    /// - Connection or transaction failures occur
    pub fn create(&mut self, template: &TaskTemplate) -> Result<()> {
        let affected = self.conn.execute(
            INSERT_TEMPLATE,
            params![template.name, template.task_name, template.comment, template.completeness],
        )?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TemplateCreateFailed));
        }

        Ok(())
    }

    /// Update an existing template
    pub fn update(&mut self, template: &TaskTemplate) -> Result<()> {
        let affected = self.conn.execute(
            UPDATE_TEMPLATE,
            params![template.name, template.task_name, template.comment, template.completeness],
        )?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TemplateNotFound(template.name.clone())));
        }

        Ok(())
    }

    /// Deletes a template permanently from the database.
    ///
    /// This method removes a template definition from the system. Note that
    /// deleting a template does not affect tasks that were previously created
    /// from that template - those tasks remain independent and unchanged.
    ///
    /// ## Impact Scope
    ///
    /// - **Template Removal**: Permanently removes the template definition
    /// - **Task Preservation**: Existing tasks created from template are unaffected
    /// - **Reference Cleanup**: Removes template from discovery and selection
    /// - **Audit Trail**: Operation can be tracked through database logs
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name identifier of the template to delete
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if deletion succeeds, or an error if the operation
    /// fails. Deleting a non-existent template is not considered an error.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut templates = Templates::new()?;
    /// templates.delete("obsolete-template")?;
    /// ```
    ///
    /// # Safety Considerations
    ///
    /// - Deletion is immediate and permanent
    /// - No confirmation prompts at this level
    /// - Callers should implement appropriate confirmation workflows
    /// - Consider exporting template definitions before deletion
    pub fn delete(&mut self, name: &str) -> Result<()> {
        let affected = self.conn.execute(DELETE_TEMPLATE, params![name])?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TemplateNotFound(name.to_string())));
        }

        Ok(())
    }

    /// Retrieves all templates from the database ordered alphabetically.
    ///
    /// This method returns a complete list of all template definitions
    /// sorted by name for consistent display in user interfaces and
    /// selection menus. The list includes all template properties and
    /// metadata for comprehensive template management.
    ///
    /// # Returns
    ///
    /// Returns a vector of all template records ordered by name, or an
    /// error if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut templates = Templates::new()?;
    /// let all_templates = templates.get_all()?;
    /// for template in all_templates {
    ///     println!("Template: {} -> {}", template.name, template.task_name);
    /// }
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// This method loads all templates into memory, which is efficient for
    /// typical template collections but may need pagination for very large
    /// numbers of templates (hundreds or thousands).
    ///
    /// # Use Cases
    ///
    /// - Template selection interfaces
    /// - Administrative template management
    /// - Template export and backup operations
    /// - System configuration displays
    pub fn get_all(&mut self) -> Result<Vec<TaskTemplate>> {
        let mut stmt = self.conn.prepare(SELECT_ALL_TEMPLATES)?;
        let template_iter = stmt.query_map([], |row| {
            Ok(TaskTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                task_name: row.get(2)?,
                comment: row.get(3)?,
                completeness: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut templates = Vec::new();
        for template in template_iter {
            templates.push(template?);
        }
        Ok(templates)
    }

    /// Finds a specific template by its unique name identifier.
    ///
    /// This method performs exact name matching to retrieve a specific
    /// template definition. It's commonly used for template editing,
    /// validation, and task creation operations that reference templates
    /// by their user-friendly names.
    ///
    /// # Arguments
    ///
    /// * `name` - Exact name of the template to retrieve (case-sensitive)
    ///
    /// # Returns
    ///
    /// Returns `Some(TaskTemplate)` if found, `None` if no matching template
    /// exists, or an error if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut templates = Templates::new()?;
    /// if let Some(template) = templates.get("daily-standup")? {
    ///     println!("Found template: {}", template.task_name);
    ///     // Use template to create a task with predefined values
    /// } else {
    ///     println!("Template 'daily-standup' not found");
    /// }
    /// ```
    ///
    /// # Name Matching
    ///
    /// - Performs exact, case-sensitive string matching
    /// - Does not support wildcards or partial matching (use search() for that)
    /// - Whitespace and special characters must match exactly
    /// - Template names are typically lowercase with hyphens for readability
    pub fn get(&mut self, name: &str) -> Result<Option<TaskTemplate>> {
        let mut stmt = self.conn.prepare(SELECT_TEMPLATE_BY_NAME)?;
        let mut template_iter = stmt.query_map(params![name], |row| {
            Ok(TaskTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                task_name: row.get(2)?,
                comment: row.get(3)?,
                completeness: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        // Extract the first (and only) result from the iterator
        match template_iter.next() {
            Some(Ok(template)) => Ok(Some(template)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Searches templates by name or task content with flexible pattern matching.
    ///
    /// This method provides fuzzy search capabilities across both template names
    /// and task names, enabling users to discover relevant templates when they
    /// don't remember exact names. It's particularly useful for interactive
    /// template selection and discovery workflows.
    ///
    /// ## Search Behavior
    ///
    /// - **Partial Matching**: Matches substrings within template or task names
    /// - **Case Insensitive**: Search is case-insensitive for user convenience
    /// - **Multiple Fields**: Searches both template name and task_name fields
    /// - **Sorted Results**: Returns results ordered alphabetically by template name
    ///
    /// ## Search Algorithm
    ///
    /// Uses SQL LIKE operator with wildcard patterns, which provides:
    /// - Substring matching anywhere in the field
    /// - Efficient execution using database indices
    /// - Consistent behavior across different database backends
    ///
    /// # Arguments
    ///
    /// * `query` - Search term to match against template and task names
    ///
    /// # Returns
    ///
    /// Returns a vector of matching templates ordered by name, or an error
    /// if the database query fails. Empty vector if no matches found.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut templates = Templates::new()?;
    ///
    /// // Find all templates related to "review"
    /// let review_templates = templates.search("review")?;
    ///
    /// // Find templates containing "standup" anywhere
    /// let standup_templates = templates.search("standup")?;
    ///
    /// for template in review_templates {
    ///     println!("Found: {} -> {}", template.name, template.task_name);
    /// }
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - LIKE queries may be slower on very large template collections
    /// - Consider indexing if template search becomes a performance bottleneck
    /// - Results are limited by available memory for the returned vector
    ///
    /// # Use Cases
    ///
    /// - Interactive template selection interfaces
    /// - Command-line template discovery
    /// - Autocomplete and suggestion systems
    /// - Template organization and categorization
    pub fn search(&mut self, query: &str) -> Result<Vec<TaskTemplate>> {
        // Prepare search pattern with wildcard matching
        let search_pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(SEARCH_TEMPLATES)?;

        let template_iter = stmt.query_map(params![search_pattern], |row| {
            Ok(TaskTemplate {
                id: row.get(0)?,
                name: row.get(1)?,
                task_name: row.get(2)?,
                comment: row.get(3)?,
                completeness: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        // Collect all matching templates
        let mut templates = Vec::new();
        for template in template_iter {
            templates.push(template?);
        }

        Ok(templates)
    }

    /// Checks if a template with the specified name exists in the database.
    ///
    /// This convenience method efficiently determines template existence
    /// without retrieving the full template data. It's useful for validation
    /// before performing operations that require existing templates or for
    /// preventing duplicate template creation.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name identifier of the template to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the template exists, `false` otherwise, or an error
    /// if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut templates = Templates::new()?;
    /// if templates.exists("daily-standup")? {
    ///     println!("Template already exists");
    /// } else {
    ///     // Safe to create new template with this name
    ///     let template = TaskTemplate::new(
    ///         "daily-standup".to_string(),
    ///         "Prepare for daily standup".to_string(),
    ///         "Review progress and plan".to_string(),
    ///         0
    ///     );
    ///     templates.create(&template)?;
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// This method is more efficient than retrieving the full template when
    /// only existence verification is needed. It internally uses the `get()`
    /// method but only checks the result without processing template data.
    ///
    /// # Use Cases
    ///
    /// - Template creation validation
    /// - User interface state management
    /// - Batch operation preprocessing
    /// - Configuration file validation
    pub fn exists(&mut self, name: &str) -> Result<bool> {
        Ok(self.get(name)?.is_some())
    }
}
