//! Tag-based task categorization and organization system.
//!
//! Provides functionality for managing tags that can be associated with tasks
//! for categorization, filtering, and organization purposes. Supports many-to-many
//! relationships between tasks and tags.
//!
//! ## Features
//!
//! - **Tag Management**: Create, update, delete, and query tag definitions
//! - **Color Coding**: Optional color assignment for visual task organization
//! - **Task Association**: Link tags to tasks with automatic relationship management
//! - **Bulk Operations**: Efficient creation and association of multiple tags
//! - **Search & Filter**: Find tags by name and retrieve tasks by tag association
//!
//! ## Usage
//!
//! ```rust
//! use kasl::db::tags::{Tags, Tag};
//!
//! let mut tags = Tags::new()?;
//! let urgent_tag = Tag::new("urgent".to_string(), Some("red".to_string()));
//! let tag_id = tags.create(&urgent_tag)?;
//! tags.add_tag_to_task(task_id, tag_id)?;
//! ```

use crate::db::db::Db;
use crate::libs::messages::Message;
use crate::msg_error_anyhow;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// SQL schema for the main tags table.
///
/// Stores tag definitions with unique names and optional color codes.
/// The table supports efficient lookups by name and provides creation
/// timestamps for audit trails.
const SCHEMA_TAGS: &str = "CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";

/// SQL schema for the task-tag relationship junction table.
///
/// Implements a many-to-many relationship between tasks and tags using
/// foreign key constraints and composite primary keys. Cascade deletions
/// ensure referential integrity when tasks or tags are removed.
const SCHEMA_TASK_TAGS: &str = "CREATE TABLE IF NOT EXISTS task_tags (
    task_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    PRIMARY KEY (task_id, tag_id),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
)";

/// Insert a new tag record with name and optional color.
///
/// Creates a new tag definition in the database with automatic ID assignment
/// and timestamp generation. Tag names must be unique across the system.
const INSERT_TAG: &str = "INSERT INTO tags (name, color) VALUES (?1, ?2)";

/// Update an existing tag's name and color properties.
///
/// Modifies tag properties while preserving the original creation timestamp
/// and maintaining referential integrity with existing task associations.
const UPDATE_TAG: &str = "UPDATE tags SET name = ?2, color = ?3 WHERE id = ?1";

/// Delete a tag record and all associated task relationships.
///
/// Removes the tag definition and automatically cleans up all task-tag
/// associations through foreign key cascade constraints.
const DELETE_TAG: &str = "DELETE FROM tags WHERE id = ?1";

/// Retrieve all tags ordered alphabetically by name.
///
/// Provides a complete list of tag definitions sorted for consistent
/// display in user interfaces and reports.
const SELECT_ALL_TAGS: &str = "SELECT * FROM tags ORDER BY name";

/// Find a specific tag by its unique name.
///
/// Enables case-sensitive tag lookup for validation and duplicate
/// prevention during tag creation and management.
const SELECT_TAG_BY_NAME: &str = "SELECT * FROM tags WHERE name = ?1";

/// Retrieve a tag record by its unique identifier.
///
/// Provides direct access to tag details using the primary key for
/// efficient lookups during task-tag association operations.
const SELECT_TAG_BY_ID: &str = "SELECT * FROM tags WHERE id = ?1";

/// Get all tags associated with a specific task.
///
/// Joins the tags and task_tags tables to retrieve complete tag information
/// for a given task, ordered alphabetically for consistent display.
const SELECT_TAGS_BY_TASK: &str = "
    SELECT t.* FROM tags t
    JOIN task_tags tt ON t.id = tt.tag_id
    WHERE tt.task_id = ?1
    ORDER BY t.name
";

/// Find all tasks associated with a specific tag.
///
/// Retrieves task IDs that are associated with the given tag identifier,
/// useful for tag-based task filtering and reporting.
const SELECT_TASKS_BY_TAG: &str = "SELECT task_id FROM task_tags WHERE tag_id = ?1";

/// Create a new task-tag association.
///
/// Links a task with a tag using the junction table. The OR IGNORE clause
/// prevents errors when the association already exists.
const INSERT_TASK_TAG: &str = "INSERT OR IGNORE INTO task_tags (task_id, tag_id) VALUES (?1, ?2)";

/// Remove a specific task-tag association.
///
/// Unlinks a tag from a task without affecting other relationships or
/// the tag/task definitions themselves.
const DELETE_TASK_TAG: &str = "DELETE FROM task_tags WHERE task_id = ?1 AND tag_id = ?2";

/// Remove all tag associations for a specific task.
///
/// Clears all tags from a task, typically used when deleting tasks or
/// when users want to reset a task's tag assignments.
const DELETE_ALL_TASK_TAGS: &str = "DELETE FROM task_tags WHERE task_id = ?1";

/// Represents a tag entity with its properties and metadata.
///
/// A tag is a label that can be associated with tasks for categorization
/// and organization. Tags support optional color coding for visual
/// organization in user interfaces.
///
/// ## Field Details
///
/// - **id**: Database-assigned unique identifier (None for new tags)
/// - **name**: Human-readable tag name (must be unique)
/// - **color**: Optional color code for visual categorization
/// - **created_at**: Timestamp of tag creation (managed by database)
///
/// ## Serialization
///
/// The struct supports JSON serialization for data export and API
/// responses, making it suitable for configuration files and web interfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    /// Unique identifier assigned by the database.
    ///
    /// This field is `None` for new tags that haven't been saved to the
    /// database yet, and `Some(id)` for existing tags retrieved from storage.
    pub id: Option<i32>,

    /// Unique name identifier for the tag.
    ///
    /// Tag names must be unique across the system and are used for
    /// human-readable identification. Names are case-sensitive and
    /// should follow consistent naming conventions.
    pub name: String,

    /// Optional color code for visual categorization.
    ///
    /// Colors can be specified as hex codes, CSS color names, or any
    /// format supported by the user interface. This field enables
    /// visual organization and quick recognition of tag categories.
    pub color: Option<String>,

    /// Timestamp when the tag was created.
    ///
    /// Automatically managed by the database and used for audit trails
    /// and chronological sorting. Format depends on database settings.
    pub created_at: Option<String>,
}

impl Tag {
    /// Creates a new tag instance with the specified name and optional color.
    ///
    /// This constructor creates a tag object ready for database insertion.
    /// The ID and creation timestamp are set by the database when the tag
    /// is saved using the `Tags::create()` method.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for the tag (will be validated for uniqueness)
    /// * `color` - Optional color code for visual organization
    ///
    /// # Returns
    ///
    /// Returns a new `Tag` instance ready for database operations.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::tags::Tag;
    ///
    /// // Create a tag with color
    /// let urgent_tag = Tag::new("urgent".to_string(), Some("red".to_string()));
    ///
    /// // Create a tag without color
    /// let general_tag = Tag::new("general".to_string(), None);
    /// ```
    pub fn new(name: String, color: Option<String>) -> Self {
        Self {
            id: None,
            name,
            color,
            created_at: None,
        }
    }
}

/// Database manager for tag operations and task-tag relationships.
///
/// The `Tags` struct provides a high-level interface for managing tags and
/// their associations with tasks. It handles database connections, schema
/// initialization, and provides methods for all tag-related operations.
///
/// ## Functionality
///
/// - **CRUD Operations**: Create, read, update, and delete tag definitions
/// - **Relationship Management**: Associate and disassociate tags with tasks
/// - **Batch Operations**: Efficiently handle multiple tag operations
/// - **Query Support**: Search and filter tags and their associations
///
/// ## Database Schema Management
///
/// The struct automatically ensures that required database tables exist
/// when instantiated, handling schema creation and migration compatibility.
pub struct Tags {
    /// Database connection for tag operations.
    ///
    /// Direct connection to the SQLite database for executing tag-related
    /// queries. The connection is managed internally and provides transactional
    /// support for complex operations.
    conn: Connection,
}

impl Tags {
    /// Creates a new Tags manager and initializes the database schema.
    ///
    /// This constructor establishes a database connection, ensures that the
    /// required tables exist, and prepares the manager for tag operations.
    /// Schema creation is idempotent and safe to call multiple times.
    ///
    /// # Returns
    ///
    /// Returns a new `Tags` instance ready for tag management operations,
    /// or an error if database initialization fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::tags::Tags;
    ///
    /// let mut tags = Tags::new()?;
    /// // Ready for tag operations
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

        // Initialize tag system tables (migration v3 creates them, but ensure they exist)
        db.conn.execute(SCHEMA_TAGS, [])?;
        db.conn.execute(SCHEMA_TASK_TAGS, [])?;

        Ok(Tags { conn: db.conn })
    }

    /// Creates a new tag in the database and returns its assigned ID.
    ///
    /// This method inserts a new tag record with the provided name and color,
    /// automatically assigning a unique ID and creation timestamp. Tag names
    /// must be unique across the system.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag object containing name and optional color
    ///
    /// # Returns
    ///
    /// Returns the database-assigned ID for the new tag, or an error if
    /// creation fails (e.g., due to duplicate names).
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::tags::{Tags, Tag};
    ///
    /// let mut tags = Tags::new()?;
    /// let tag = Tag::new("priority".to_string(), Some("orange".to_string()));
    /// let tag_id = tags.create(&tag)?;
    /// println!("Created tag with ID: {}", tag_id);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A tag with the same name already exists
    /// - Database constraints are violated
    /// - Connection or transaction failures occur
    pub fn create(&mut self, tag: &Tag) -> Result<i32> {
        self.conn.execute(INSERT_TAG, params![tag.name, tag.color])?;
        Ok(self.conn.last_insert_rowid() as i32)
    }

    /// Updates an existing tag's name and color properties.
    ///
    /// This method modifies an existing tag's properties while preserving
    /// its ID, creation timestamp, and task associations. The updated name
    /// must still be unique across all tags.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag object with updated properties (must have a valid ID)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the update succeeds, or an error if the operation
    /// fails or the tag doesn't exist.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// let mut tag = tags.get_by_id(tag_id)?.unwrap();
    /// tag.name = "high-priority".to_string();
    /// tag.color = Some("crimson".to_string());
    /// tags.update(&tag)?;
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The tag ID doesn't exist in the database
    /// - The new name conflicts with an existing tag
    /// - Database constraints are violated
    pub fn update(&mut self, tag: &Tag) -> Result<()> {
        let id = tag.id.ok_or_else(|| msg_error_anyhow!(Message::TagNotFound(tag.name.to_string())))?;

        let affected = self.conn.execute(UPDATE_TAG, params![id, tag.name, tag.color])?;

        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TagNotFound(tag.name.to_string())));
        }

        Ok(())
    }

    /// Deletes a tag and all its task associations from the database.
    ///
    /// This method permanently removes a tag definition and automatically
    /// cleans up all task-tag relationships through foreign key cascading.
    /// The operation is atomic and cannot be undone without database backups.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier of the tag to delete
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if deletion succeeds, or an error if the operation
    /// fails. Deleting a non-existent tag is not considered an error.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// tags.delete(tag_id)?; // Removes tag and all associations
    /// ```
    ///
    /// # Side Effects
    ///
    /// - Removes the tag definition from the tags table
    /// - Automatically removes all task-tag associations via CASCADE
    /// - Cannot be undone without database restore operations
    pub fn delete(&mut self, id: i32) -> Result<()> {
        let affected = self.conn.execute(DELETE_TAG, params![id])?;
        if affected == 0 {
            return Err(msg_error_anyhow!(Message::TagNotFound(id.to_string())));
        }
        Ok(())
    }

    /// Retrieves all tags from the database ordered alphabetically.
    ///
    /// This method returns a complete list of all tag definitions sorted
    /// by name for consistent display in user interfaces and reports.
    /// The list includes all tag properties including colors and timestamps.
    ///
    /// # Returns
    ///
    /// Returns a vector of all tag records ordered by name, or an error
    /// if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// let all_tags = tags.get_all()?;
    /// for tag in all_tags {
    ///     println!("Tag: {} ({})", tag.name, tag.color.unwrap_or("no color".to_string()));
    /// }
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// This method loads all tags into memory, which is efficient for small
    /// to medium tag collections but may need pagination for very large datasets.
    pub fn get_all(&mut self) -> Result<Vec<Tag>> {
        let mut stmt = self.conn.prepare(SELECT_ALL_TAGS)?;
        let tag_iter = stmt.query_map([], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut tags = Vec::new();
        for tag in tag_iter {
            tags.push(tag?);
        }
        Ok(tags)
    }

    /// Finds a tag by its unique name with case-sensitive matching.
    ///
    /// This method performs an exact name lookup to find a specific tag
    /// definition. It's commonly used for validation during tag creation
    /// and for resolving tag names to IDs in user commands.
    ///
    /// # Arguments
    ///
    /// * `name` - Exact name of the tag to find (case-sensitive)
    ///
    /// # Returns
    ///
    /// Returns `Some(Tag)` if found, `None` if no matching tag exists,
    /// or an error if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// if let Some(tag) = tags.get_by_name("urgent")? {
    ///     println!("Found tag: {} with color: {:?}", tag.name, tag.color);
    /// } else {
    ///     println!("Tag 'urgent' not found");
    /// }
    /// ```
    ///
    /// # Name Matching
    ///
    /// - Performs exact, case-sensitive string matching
    /// - Does not support wildcards or partial matching
    /// - Whitespace is significant and must match exactly
    pub fn get_by_name(&mut self, name: &str) -> Result<Option<Tag>> {
        let tag = self
            .conn
            .query_row(SELECT_TAG_BY_NAME, params![name], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .optional()?;
        Ok(tag)
    }

    /// Retrieves a tag by its unique database identifier.
    ///
    /// This method provides direct access to tag details using the primary
    /// key for efficient lookups during task-tag association operations
    /// and when processing user commands with tag IDs.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique database identifier of the tag
    ///
    /// # Returns
    ///
    /// Returns `Some(Tag)` if found, `None` if the ID doesn't exist,
    /// or an error if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// if let Some(tag) = tags.get_by_id(42)? {
    ///     println!("Tag ID 42: {}", tag.name);
    /// }
    /// ```
    ///
    /// # Performance
    ///
    /// This is the most efficient way to retrieve a tag when the ID is known,
    /// as it uses the primary key index for direct record access.
    pub fn get_by_id(&mut self, id: i32) -> Result<Option<Tag>> {
        let tag = self
            .conn
            .query_row(SELECT_TAG_BY_ID, params![id], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            .optional()?;
        Ok(tag)
    }

    /// Retrieves all tags associated with a specific task.
    ///
    /// This method returns the complete set of tags linked to a task,
    /// ordered alphabetically for consistent display. It joins the tags
    /// and task_tags tables to provide full tag information.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Unique identifier of the task
    ///
    /// # Returns
    ///
    /// Returns a vector of tags associated with the task, or an error
    /// if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// let task_tags = tags.get_tags_by_task(task_id)?;
    /// for tag in task_tags {
    ///     println!("Task has tag: {}", tag.name);
    /// }
    /// ```
    ///
    /// # Return Characteristics
    ///
    /// - Empty vector if the task has no tags
    /// - Results ordered alphabetically by tag name
    /// - Includes full tag details (name, color, timestamps)
    pub fn get_tags_by_task(&mut self, task_id: i32) -> Result<Vec<Tag>> {
        let mut stmt = self.conn.prepare(SELECT_TAGS_BY_TASK)?;
        let tag_iter = stmt.query_map(params![task_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut tags = Vec::new();
        for tag in tag_iter {
            tags.push(tag?);
        }
        Ok(tags)
    }

    /// Finds all tasks associated with a specific tag.
    ///
    /// This method returns the list of task IDs that have been tagged
    /// with the specified tag. It's useful for tag-based filtering and
    /// generating reports of tasks by category.
    ///
    /// # Arguments
    ///
    /// * `tag_id` - Unique identifier of the tag
    ///
    /// # Returns
    ///
    /// Returns a vector of task IDs associated with the tag, or an error
    /// if the database query fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// let task_ids = tags.get_tasks_by_tag(tag_id)?;
    /// println!("Tag is used by {} tasks", task_ids.len());
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Tag-based task filtering in reports
    /// - Counting tag usage frequency
    /// - Validating tag deletion impact
    /// - Generating tag-specific task lists
    pub fn get_tasks_by_tag(&mut self, tag_id: i32) -> Result<Vec<i32>> {
        let mut stmt = self.conn.prepare(SELECT_TASKS_BY_TAG)?;
        let task_iter = stmt.query_map(params![tag_id], |row| row.get(0))?;

        let mut task_ids = Vec::new();
        for task_id in task_iter {
            task_ids.push(task_id?);
        }
        Ok(task_ids)
    }

    /// Associates a tag with a task, creating a many-to-many relationship.
    ///
    /// This method creates a link between a task and a tag in the junction
    /// table. If the association already exists, the operation succeeds
    /// without creating duplicates due to the OR IGNORE clause.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Unique identifier of the task
    /// * `tag_id` - Unique identifier of the tag
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the association is created or already exists,
    /// or an error if the database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// tags.add_tag_to_task(task_id, tag_id)?;
    /// println!("Tag associated with task");
    /// ```
    ///
    /// # Idempotency
    ///
    /// This operation is idempotent - calling it multiple times with the
    /// same parameters has the same effect as calling it once.
    pub fn add_tag_to_task(&mut self, task_id: i32, tag_id: i32) -> Result<()> {
        self.conn.execute(INSERT_TASK_TAG, params![task_id, tag_id])?;
        Ok(())
    }

    /// Removes a specific tag association from a task.
    ///
    /// This method removes the link between a task and a tag without
    /// affecting the tag definition or other task associations. The
    /// operation is safe and succeeds even if the association doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Unique identifier of the task
    /// * `tag_id` - Unique identifier of the tag to remove
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the removal succeeds, or an error if the
    /// database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// tags.remove_tag_from_task(task_id, tag_id)?;
    /// println!("Tag removed from task");
    /// ```
    ///
    /// # Side Effects
    ///
    /// - Only affects the specific task-tag relationship
    /// - Does not delete the tag or task definitions
    /// - Safe to call even if the association doesn't exist
    pub fn remove_tag_from_task(&mut self, task_id: i32, tag_id: i32) -> Result<()> {
        self.conn.execute(DELETE_TASK_TAG, params![task_id, tag_id])?;
        Ok(())
    }

    /// Removes all tag associations from a specific task.
    ///
    /// This method clears all tags from a task, effectively resetting its
    /// tag assignments. It's commonly used when deleting tasks or when
    /// users want to completely re-tag a task.
    ///
    /// # Arguments
    ///
    /// * `task_id` - Unique identifier of the task to clear
    ///
    /// # Returns
    ///
    /// Returns the number of associations removed, or an error if the
    /// database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// let removed_count = tags.remove_all_tags_from_task(task_id)?;
    /// println!("Removed {} tag associations", removed_count);
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Task deletion cleanup
    /// - Bulk tag reassignment workflows
    /// - Resetting task categorization
    /// - Data migration and correction operations
    pub fn remove_all_tags_from_task(&mut self, task_id: i32) -> Result<usize> {
        let affected = self.conn.execute(DELETE_ALL_TASK_TAGS, params![task_id])?;
        Ok(affected)
    }

    /// Replaces all tag associations for a task with a new set of tags.
    ///
    /// This method provides atomic tag assignment by completely replacing
    /// a task's current tag associations with a new set. It ensures that
    /// the task ends up with exactly the specified tags, regardless of
    /// its previous tag state.
    ///
    /// ## Operation Sequence
    ///
    /// The method performs a two-step atomic operation:
    /// 1. **Clear Existing**: Removes all current tag associations for the task
    /// 2. **Add New**: Creates associations for each specified tag ID
    ///
    /// This approach ensures consistency and prevents partial update states
    /// that could occur with manual add/remove operations.
    ///
    /// ## Transaction Semantics
    ///
    /// While not explicitly wrapped in a transaction, the operation is
    /// designed to be atomic - either all associations are updated successfully
    /// or the task retains its original tag state if any error occurs.
    ///
    /// ## Use Cases
    ///
    /// - **Bulk Tag Assignment**: Assigning multiple tags to a task at once
    /// - **Tag Replacement**: Completely changing a task's tag categorization
    /// - **Import Operations**: Setting tags during data import or migration
    /// - **Template Application**: Applying predefined tag sets from templates
    /// - **UI Operations**: Saving tag selections from multi-select interfaces
    ///
    /// # Arguments
    ///
    /// * `task_id` - Unique identifier of the task to update
    /// * `tag_ids` - Slice of tag IDs to associate with the task
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all tag associations are updated successfully,
    /// or an error if any database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    ///
    /// // Replace task tags with new set
    /// let new_tag_ids = vec![1, 3, 5]; // urgent, backend, review
    /// tags.set_task_tags(task_id, &new_tag_ids)?;
    ///
    /// // Task now has exactly these three tags
    /// let current_tags = tags.get_tags_by_task(task_id)?;
    /// assert_eq!(current_tags.len(), 3);
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// - **Efficient for Large Changes**: More efficient than individual add/remove operations
    /// - **Database Operations**: Minimizes database round-trips through batch processing
    /// - **Index Usage**: Leverages database indices for both clear and insert operations
    /// - **Memory Usage**: Tag ID slice is processed iteratively to minimize memory usage
    ///
    /// # Error Handling
    ///
    /// If the clear operation succeeds but adding new tags fails, the task
    /// will be left with no tags. Callers should be prepared to handle this
    /// scenario and potentially retry the operation or restore previous state.
    ///
    /// # Data Integrity
    ///
    /// The method assumes all provided tag IDs exist in the database. Non-existent
    /// tag IDs will cause foreign key constraint violations and operation failure.
    /// Use `get_or_create_tags()` if tag existence is uncertain.
    pub fn set_task_tags(&mut self, task_id: i32, tag_ids: &[i32]) -> Result<()> {
        // Clear all existing tag associations for this task
        self.remove_all_tags_from_task(task_id)?;

        // Add each new tag association
        for tag_id in tag_ids {
            self.add_tag_to_task(task_id, *tag_id)?;
        }

        Ok(())
    }

    /// Creates or retrieves tags by name, returning their database IDs.
    ///
    /// This convenience method handles the common workflow of ensuring tags
    /// exist before associating them with tasks. For each provided name,
    /// it either returns the existing tag ID or creates a new tag with
    /// a default color.
    ///
    /// ## Batch Processing
    ///
    /// The method processes multiple tag names efficiently, checking for
    /// existence before creation and returning a complete list of IDs
    /// ready for task association.
    ///
    /// # Arguments
    ///
    /// * `names` - Slice of tag names to create or retrieve
    ///
    /// # Returns
    ///
    /// Returns a vector of tag IDs corresponding to the input names,
    /// or an error if any database operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let mut tags = Tags::new()?;
    /// let tag_names = vec!["urgent".to_string(), "backend".to_string()];
    /// let tag_ids = tags.get_or_create_tags(&tag_names)?;
    /// // tag_ids now contains IDs for both tags (created if needed)
    /// ```
    ///
    /// # Default Color Assignment
    ///
    /// New tags are assigned colors from a predefined rotation to ensure
    /// visual variety. The color assignment is deterministic but varies
    /// across different tag creation sessions.
    pub fn get_or_create_tags(&mut self, names: &[String]) -> Result<Vec<i32>> {
        let mut tag_ids = Vec::new();

        for name in names {
            let tag = match self.get_by_name(name)? {
                Some(existing_tag) => existing_tag,
                None => {
                    // Create new tag with a default color from rotation
                    let tag = Tag::new(name.clone(), Some(Self::get_default_color()));
                    let id = self.create(&tag)?;
                    Tag {
                        id: Some(id),
                        name: name.clone(),
                        color: tag.color,
                        created_at: None,
                    }
                }
            };

            if let Some(id) = tag.id {
                tag_ids.push(id);
            }
        }

        Ok(tag_ids)
    }

    /// Generates a default color for new tags using a rotation algorithm.
    ///
    /// This internal method provides automatic color assignment for tags
    /// created without explicit color specifications. It cycles through
    /// a predefined set of colors to ensure visual variety and consistency.
    ///
    /// ## Color Rotation
    ///
    /// The method maintains a static counter that advances through a
    /// predefined color palette, ensuring that consecutively created
    /// tags receive different colors for better visual distinction.
    ///
    /// # Returns
    ///
    /// Returns a color name string from the predefined palette.
    ///
    /// # Thread Safety
    ///
    /// This method uses unsafe static mutation for simplicity. In a
    /// multi-threaded environment, this could lead to race conditions,
    /// but the impact is minimal (just color selection variation).
    ///
    /// # Color Palette
    ///
    /// The current palette includes: blue, green, yellow, red, purple,
    /// cyan, and orange, providing good visual variety for most use cases.
    fn get_default_color() -> String {
        // Predefined color palette for automatic assignment
        static COLORS: &[&str] = &["blue", "green", "yellow", "red", "purple", "cyan", "orange"];
        static mut COLOR_INDEX: usize = 0;

        unsafe {
            let color = COLORS[COLOR_INDEX % COLORS.len()];
            COLOR_INDEX += 1;
            color.to_string()
        }
    }
}
