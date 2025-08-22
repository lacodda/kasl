//! Database layer for the kasl application.
//!
//! Provides a complete data persistence layer built on SQLite, offering type-safe
//! database operations for all application entities. Implements a migration system
//! for schema evolution and provides specialized modules for different data types.
//!
//! ## Features
//!
//! - **Core Infrastructure**: Connection management and migrations
//! - **Time Tracking**: Workdays and pause records for activity monitoring
//! - **Task Management**: Tasks, templates, and organizational features
//! - **Productivity Analytics**: Data aggregation and reporting support
//!
//! ## Usage
//!
//! ```rust
//! use kasl::db::{db::Db, tasks::Tasks, workdays::Workdays};
//! use kasl::libs::task::Task;
//!
//! let db = Db::new()?;
//! let mut tasks = Tasks::new()?;
//! let task = Task::new("Review code", "Check PR #123", Some(75));
//! tasks.insert(&task)?;
//! ```
//!
//! ```rust
//! use kasl::db::{tags::Tags, templates::Templates};
//!
//! // Create and manage tags
//! let mut tags = Tags::new()?;
//! let tag_id = tags.create(&Tag::new("urgent".to_string(), Some("red".to_string())))?;
//!
//! // Work with task templates
//! let mut templates = Templates::new()?;
//! let template = TaskTemplate::new(
//!     "daily-standup".to_string(),
//!     "Attend daily standup meeting".to_string(),
//!     "Team sync and planning".to_string(),
//!     100
//! );
//! templates.create(&template)?;
//! ```
//!
//! ## Performance Considerations
//!
//! ### Indexing Strategy
//! - **Temporal Queries**: Optimized indexes on timestamp columns
//! - **Relationship Lookups**: Efficient foreign key index coverage
//! - **Search Operations**: Selective indexes for common query patterns
//!
//! ### Connection Management
//! - **Connection Reuse**: Long-lived connections for better performance
//! - **Transaction Batching**: Grouped operations for improved throughput
//! - **Statement Preparation**: Cached prepared statements for repeated queries
//!
//! ### Data Volume Handling
//! - **Pagination Support**: Efficient handling of large result sets
//! - **Selective Loading**: Fetch only required fields for large tables
//! - **Archive Strategy**: Data retention policies for long-term usage
//!
//! ## Migration Best Practices
//!
//! ### Schema Evolution
//! ```rust
//! // Adding a new column (backward compatible)
//! tx.execute("ALTER TABLE tasks ADD COLUMN priority INTEGER DEFAULT 1", [])?;
//!
//! // Creating new indexes for performance
//! tx.execute("CREATE INDEX idx_tasks_priority ON tasks(priority)", [])?;
//!
//! // Adding new tables with proper foreign keys
//! tx.execute("CREATE TABLE task_dependencies (
//!     id INTEGER PRIMARY KEY,
//!     task_id INTEGER NOT NULL,
//!     depends_on INTEGER NOT NULL,
//!     FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
//!     FOREIGN KEY (depends_on) REFERENCES tasks(id) ON DELETE CASCADE
//! )", [])?;
//! ```
//!
//! ### Version Control Integration
//! - **Sequential Versioning**: Each migration increments the version number
//! - **Descriptive Names**: Clear migration names describing the change
//! - **Testing Requirements**: All migrations must be tested before deployment
//! - **Rollback Planning**: Consider rollback implications for schema changes
//!
//! ## Platform-Specific Considerations
//!
//! ### File System Integration
//! - **Windows**: Handles path length limitations and permission requirements
//! - **macOS**: Integrates with application sandbox restrictions
//! - **Linux**: Follows XDG base directory specifications
//!
//! ### Backup and Recovery
//! - **Export Functionality**: Complete data export in multiple formats
//! - **Import Validation**: Schema validation during data import
//! - **Corruption Recovery**: Database integrity checks and repair options
//!
//! ## Development and Debugging
//!
//! ### Debug Features
//! - **Migration Inspection**: View current schema version and history
//! - **Query Logging**: Optional SQL query logging for performance analysis
//! - **Connection Monitoring**: Track active connections and lock contention
//!
//! ### Testing Support
//! - **In-Memory Databases**: Fast test execution with temporary databases
//! - **Fixture Management**: Consistent test data setup and teardown
//! - **Migration Testing**: Automated testing of schema changes

/// Core database connection and initialization module.
///
/// Provides the fundamental `Db` struct that manages SQLite connections,
/// applies migrations, and ensures proper database configuration.
pub mod db;

/// Database schema migration system.
///
/// Handles versioned schema changes, tracks migration history, and provides
/// development-time migration management commands.
pub mod migrations;

/// Manual break period management.
///
/// Handles user-defined break periods for productivity optimization, allowing
/// manual addition of intentional breaks to improve productivity calculations.
pub mod breaks;

/// Break and pause tracking operations.
///
/// Manages records of user inactivity periods, break times, and interruptions
/// during work sessions for productivity analysis.
pub mod pauses;

/// Task categorization and organization system.
///
/// Provides tag-based organization for tasks, including many-to-many
/// relationships and color-coded categorization.
pub mod tags;

/// Core task management operations.
///
/// Handles CRUD operations for user tasks, including creation, updates,
/// completion tracking, and various filtering and search capabilities.
pub mod tasks;

/// Reusable task template system.
///
/// Manages pre-defined task templates for common activities, enabling
/// quick task creation from standardized patterns.
pub mod templates;

/// Daily work session tracking.
///
/// Records work session start/end times, manages workday lifecycle, and
/// provides the foundation for time tracking and productivity reporting.
pub mod workdays;
