# Kasl Documentation Style Guide

This style guide defines standards for writing comments, module documentation, and API documentation for the kasl project.

## 📋 Table of Contents

- [General Principles](#general-principles)
- [Module Documentation](#module-documentation)
- [Function Documentation](#function-documentation)
- [Struct Documentation](#struct-documentation)
- [Field Documentation](#field-documentation)
- [Constants and SQL](#constants-and-sql)
- [Test Documentation](#test-documentation)
- [Examples](#examples)

## 🎯 General Principles

### Core Rules

1. **Brevity and Clarity** - Comments should be informative but not excessive
2. **Consistency** - Use consistent patterns throughout the project
3. **Relevance** - Documentation should match the code
4. **Practicality** - Focus on what developers need to know

### Terminology

Use consistent terminology:
- **Task** - A unit of work
- **Workday** - A work session
- **Pause** - A break in work
- **Tag** - Task categorization
- **Template** - Task template

## 📚 Module Documentation

### Standard Structure

```rust
//! [Brief module name]
//! 
//! [Brief description in one sentence]
//! 
//! ## Features
//! 
//! - [List of main capabilities]
//! 
//! ## Usage
//! 
//! [Usage examples]
//! 
//! ## Architecture
//! 
//! [Architecture description, if applicable]
```

### Examples

**✅ Good:**
```rust
//! Task management database operations.
//! 
//! Provides core functionality for managing tasks within the kasl application.
//! Handles all database interactions for task creation, modification, deletion,
//! and retrieval with support for advanced filtering, tagging, and relationship management.
//! 
//! ## Features
//! 
//! - **CRUD Operations**: Complete Create, Read, Update, Delete functionality
//! - **Advanced Filtering**: Multi-criteria task querying with date, completion, and tag filters
//! - **Batch Operations**: Efficient bulk deletion and modification operations
//! - **Tag Integration**: Seamless integration with the tagging system for categorization
//! 
//! ## Usage
//! 
//! ```rust
//! use kasl::db::tasks::{Tasks, TaskFilter};
//! use kasl::libs::task::Task;
//! 
//! let mut tasks = Tasks::new()?;
//! let task = Task::new("Review code", "Check PR #123", Some(75));
//! tasks.insert(&task)?;
//! ```
```

**❌ Bad:**
```rust
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
```

## 🔧 Function Documentation

### Standard Structure

```rust
/// [Brief function description]
///
/// [Detailed description if necessary]
///
/// # Arguments
///
/// * `param` - [parameter description]
///
/// # Returns
///
/// [return value description]
///
/// # Errors
///
/// [possible errors description]
///
/// # Examples
///
/// ```rust
/// [usage example]
/// ```
```

### Examples

**✅ Good:**
```rust
/// Performs authentication and returns a session identifier.
///
/// Handles API authentication using stored credentials and returns a session ID
/// for subsequent API calls.
///
/// # Returns
///
/// Session identifier on success, error on failure
///
/// # Errors
///
/// Returns an error if:
/// - Network connection fails
/// - Credentials are invalid
/// - API returns an unexpected response format
```

**❌ Bad:**
```rust
/// Performs authentication and returns a session identifier.
///
/// This method handles the actual API authentication process using stored
/// credentials. The returned session ID can be used for subsequent API calls.
///
/// # Returns
///
/// * `Result<String>` - Session identifier on success, error on failure
///
/// # Errors
///
/// Returns an error if:
/// - Network connection fails
/// - Credentials are invalid
/// - API returns an unexpected response format
```

## 🏗️ Struct Documentation

### Standard Structure

```rust
/// [Brief struct description]
///
/// [Detailed description of purpose and usage]
///
/// ## Fields
///
/// - `field1`: [field description]
/// - `field2`: [field description]
///
/// ## Usage
///
/// [usage examples]
```

### Examples

**✅ Good:**
```rust
/// Task representation with all associated metadata.
///
/// Represents a single work item with progress tracking, categorization,
/// and relationship capabilities.
///
/// ## Fields
///
/// - `id`: Database primary key for unique identification
/// - `name`: Human-readable task title
/// - `completeness`: Progress percentage (0-100)
/// - `tags`: Collection of categorization labels
```

## 📝 Field Documentation

### Standard Structure

```rust
/// [Brief field description]
///
/// [Detailed description of purpose, values, and usage]
///
/// ## Values
///
/// - `value1`: [value description]
/// - `value2`: [value description]
///
/// ## Examples
///
/// [usage examples]
```

### Examples

**✅ Good:**
```rust
/// Database primary key for task identification.
///
/// Automatically assigned when the task is first saved. Used for database
/// queries and cross-referencing with other application data.
///
/// ## Values
///
/// - `Some(id)`: Task exists in database with assigned ID
/// - `None`: New task not yet saved to database
pub id: Option<i32>,

/// Task completion percentage indicating progress from 0% to 100%.
///
/// Tracks the task's progress through its lifecycle, providing quantitative
/// measurement of work completion.
///
/// ## Values
///
/// - `Some(0)`: Task not started
/// - `Some(1-99)`: Task in progress
/// - `Some(100)`: Task completed
/// - `None`: Progress not tracked or unknown
pub completeness: Option<i32>,
```

**❌ Bad:**
```rust
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
```

## 🔗 Constants and SQL

### SQL Constants

```rust
/// SQL schema for the [table_name] table.
///
/// Defines the complete structure for storing [description] with support for:
/// - [feature 1]
/// - [feature 2]
/// - [feature 3]
const SCHEMA_[TABLE]: &str = "...";

/// [Operation description] query.
///
/// [Detailed description of purpose and usage]
const [OPERATION]_[TABLE]: &str = "...";
```

### Examples

**✅ Good:**
```rust
/// SQL schema for the tasks table.
///
/// Defines the complete structure for storing task information with support for:
/// - Unique identification and hierarchical relationships
/// - Temporal tracking with automatic timestamp generation
/// - Progress monitoring through completion percentages
const SCHEMA_TASKS: &str = "...";

/// Insert a new task with full field specification.
///
/// Creates a task record with automatic timestamp generation and immediate
/// ID return for further operations.
const INSERT_TASK: &str = "...";
```

## 🧪 Test Documentation

### Standard Structure

```rust
/// [Brief test description]
///
/// [Detailed description of what is being tested and how]
///
/// ## Test Setup
///
/// [test setup description]
///
/// ## Assertions
///
/// [assertions description]
```

### Examples

**✅ Good:**
```rust
/// Test context providing isolated environment for configuration tests.
///
/// Sets up temporary directories and environment variables to ensure
/// tests don't interfere with each other or the user's actual configuration.
struct ConfigTestContext {
    _temp_dir: TempDir,
    // ... other fields
}

/// Verifies that default configuration is created correctly.
///
/// Ensures that when no configuration exists, the system creates
/// appropriate default values for all configuration sections.
#[test]
fn test_default_config() {
    // ... test implementation
}
```

**❌ Bad:**
```rust
/// Test context to ensure a clean environment for each config test.
/// It sets up a temporary directory to act as the user's home/appdata directory.
struct ConfigTestContext {
    _temp_dir: TempDir,
    // ... other fields
}

#[test]
fn test_default_config(_ctx: &mut ConfigTestContext) {
    // ... test implementation
}
```

## 📖 Examples

### Complete Module Example

```rust
//! Task management database operations.
//! 
//! Provides core functionality for managing tasks within the kasl application.
//! Handles all database interactions for task creation, modification, deletion,
//! and retrieval with support for advanced filtering, tagging, and relationship management.
//! 
//! ## Features
//! 
//! - **CRUD Operations**: Complete Create, Read, Update, Delete functionality
//! - **Advanced Filtering**: Multi-criteria task querying with date, completion, and tag filters
//! - **Batch Operations**: Efficient bulk deletion and modification operations
//! - **Tag Integration**: Seamless integration with the tagging system for categorization
//! 
//! ## Usage
//! 
//! ```rust
//! use kasl::db::tasks::{Tasks, TaskFilter};
//! use kasl::libs::task::Task;
//! 
//! let mut tasks = Tasks::new()?;
//! let task = Task::new("Review code", "Check PR #123", Some(75));
//! tasks.insert(&task)?;
//! 
//! let today_tasks = tasks.fetch(TaskFilter::Date(Local::now().date_naive()))?;
//! ```

/// Task representation with all associated metadata.
///
/// Represents a single work item with progress tracking, categorization,
/// and relationship capabilities.
#[derive(Debug, Clone)]
pub struct Task {
    /// Database primary key for task identification.
    ///
    /// Automatically assigned when the task is first saved. Used for database
    /// queries and cross-referencing with other application data.
    ///
    /// ## Values
    ///
    /// - `Some(id)`: Task exists in database with assigned ID
    /// - `None`: New task not yet saved to database
    pub id: Option<i32>,

    /// Task completion percentage indicating progress from 0% to 100%.
    ///
    /// Tracks the task's progress through its lifecycle, providing quantitative
    /// measurement of work completion.
    ///
    /// ## Values
    ///
    /// - `Some(0)`: Task not started
    /// - `Some(1-99)`: Task in progress
    /// - `Some(100)`: Task completed
    /// - `None`: Progress not tracked or unknown
    pub completeness: Option<i32>,
}

impl Task {
    /// Creates a new task with the specified properties.
    ///
    /// Initializes a task with the given name, comment, and completion status.
    /// The task will be assigned an ID when saved to the database.
    ///
    /// # Arguments
    ///
    /// * `name` - Task title or description
    /// * `comment` - Optional detailed description
    /// * `completeness` - Initial completion percentage
    ///
    /// # Examples
    ///
    /// ```rust
    /// let task = Task::new("Fix bug", "Critical security issue", Some(0));
    /// ```
    pub fn new(name: &str, comment: &str, completeness: Option<i32>) -> Self {
        // ... implementation
    }
}

/// SQL schema for the tasks table.
///
/// Defines the complete structure for storing task information with support for:
/// - Unique identification and hierarchical relationships
/// - Temporal tracking with automatic timestamp generation
/// - Progress monitoring through completion percentages
const SCHEMA_TASKS: &str = "CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER NOT NULL PRIMARY KEY,
    task_id INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 0,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 100,
    excluded_from_search BOOLEAN NOT NULL ON CONFLICT REPLACE DEFAULT FALSE
);";
```

## 🔄 Implementation Process

### Priorities

1. **High Priority**: Standardize module comments (`//!`)
2. **Medium Priority**: Unify function comments (`///`)
3. **Low Priority**: Update test documentation

### Implementation Recommendations

1. **Start with new files** - Apply standards to new code
2. **Gradual updates** - Update existing files when they are modified
3. **Compliance checking** - Use `cargo doc` to verify documentation
4. **Code review** - Include documentation review in the review process

## 📚 Additional Resources

- [Rust Documentation Guidelines](https://doc.rust-lang.org/book/ch14-02-publishing-to-crates-io.html#documentation-comments)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/documentation.html)
- [Cargo Book - Documentation](https://doc.rust-lang.org/cargo/reference/publishing.html#documentation)

---

*This document should be updated as the project evolves and documentation needs change.*

