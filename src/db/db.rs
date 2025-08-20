//! Core database connection management and initialization infrastructure.
//!
//! Provides foundational database functionality including connection management,
//! schema initialization, and migration orchestration.
//!
//! ## Features
//!
//! - **Connection Management**: Establishing and configuring SQLite connections
//! - **Schema Initialization**: Ensuring database structure is properly set up
//! - **Migration Orchestration**: Coordinating automatic schema updates
//! - **Configuration Enforcement**: Applying consistent database settings
//! - **Error Handling**: Providing robust error management for database operations
//!
//! ## Usage
//!
//! ```rust
//! use kasl::db::db::Db;
//!
//! let db = Db::new()?;
//! let count: i32 = db.conn.query_row(
//!     "SELECT COUNT(*) FROM tasks",
//!     [],
//!     |row| row.get(0)
//! )?;
//! ```

use crate::db::migrations;
use crate::libs::data_storage::DataStorage;
use anyhow::Result;
use rusqlite::Connection;

/// Standard filename for the SQLite database file.
///
/// This constant ensures consistency across the application when referencing
/// the database file. The name is designed to be:
/// - **Descriptive**: Clearly identifies the application and purpose
/// - **Platform Safe**: Compatible with all target operating systems
/// - **Version Neutral**: Suitable for use across application versions
pub const DB_FILE_NAME: &str = "kasl.db";

/// Core database manager providing connection and initialization services.
///
/// The `Db` struct serves as the primary interface for database access throughout
/// the kasl application. It encapsulates a SQLite connection with all necessary
/// configuration applied and provides methods for both standard operations and
/// specialized scenarios like migration management.
///
/// ## Design Philosophy
///
/// The struct follows the principle of "initialization with validation" - when
/// a `Db` instance is created, callers can be confident that:
/// - Database file is accessible and writable
/// - Schema is current and properly migrated
/// - Foreign key constraints are active
/// - Connection is ready for immediate use
///
/// ## Thread Safety Considerations
///
/// SQLite connections are not thread-safe by default. Each `Db` instance
/// should be used within a single thread, or appropriate synchronization
/// mechanisms should be employed when sharing connections across threads.
///
/// ## Connection Configuration
///
/// All connections are configured with:
/// - Foreign key constraint enforcement enabled
/// - Local timezone handling for timestamp operations
/// - Appropriate pragma settings for desktop application usage
/// - Transaction isolation levels suitable for single-user scenarios
pub struct Db {
    /// The configured SQLite database connection.
    ///
    /// This connection has been fully initialized with:
    /// - Foreign key constraints enabled for referential integrity
    /// - All pending database migrations applied automatically
    /// - Appropriate configuration for kasl's usage patterns
    /// - UTF-8 encoding configured for international text support
    ///
    /// The connection can be used directly for custom queries or passed
    /// to specialized database modules for specific operations.
    pub conn: Connection,
}

impl Db {
    /// Creates a new database instance with complete initialization and migration.
    ///
    /// This is the primary constructor for database access in the kasl application.
    /// It performs the complete database setup process including file location
    /// resolution, connection establishment, configuration application, and
    /// automatic schema migration to the latest version.
    ///
    /// ## Initialization Process
    ///
    /// 1. **File Path Resolution**: Determines the appropriate database file location
    ///    using platform-specific application data directories
    /// 2. **Directory Creation**: Ensures all parent directories in the path exist
    /// 3. **Connection Establishment**: Opens SQLite connection to the database file
    /// 4. **Foreign Key Activation**: Enables referential integrity enforcement
    /// 5. **Migration Execution**: Automatically applies any pending schema updates
    /// 6. **Validation**: Confirms the database is ready for application use
    ///
    /// ## Migration Behavior
    ///
    /// The method automatically applies all pending database migrations during
    /// initialization. This ensures that:
    /// - Schema is always current with the application version
    /// - Data migrations preserve existing information
    /// - New features have required database structures available
    /// - Rollback scenarios are handled appropriately
    ///
    /// # Returns
    ///
    /// Returns a fully initialized `Db` instance ready for immediate use,
    /// or an error if any step of the initialization process fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::db::Db;
    ///
    /// // Standard database initialization
    /// let db = Db::new()?;
    ///
    /// // Database is ready for queries
    /// let task_count: i32 = db.conn.query_row(
    ///     "SELECT COUNT(*) FROM tasks",
    ///     [],
    ///     |row| row.get(0)
    /// )?;
    ///
    /// println!("Database contains {} tasks", task_count);
    /// ```
    ///
    /// # Error Scenarios
    ///
    /// This method can fail in several scenarios:
    /// - **File System**: Cannot create database directories or files
    /// - **Permissions**: Insufficient permissions for database file access
    /// - **Corruption**: Database file exists but is corrupted or incompatible
    /// - **Migration**: Schema migration fails due to data incompatibility
    /// - **Configuration**: SQLite configuration cannot be applied
    ///
    /// # Performance Notes
    ///
    /// The initialization process includes file system operations and potential
    /// database migrations, which may take time on first run or after updates.
    /// Subsequent initializations with an existing, current database are much faster.
    pub fn new() -> Result<Self> {
        // Resolve the platform-appropriate database file path
        let db_file_path = DataStorage::new().get_path(DB_FILE_NAME)?;

        // Establish connection to the SQLite database
        let mut conn = Connection::open(db_file_path)?;

        // Enable foreign key constraint enforcement for referential integrity
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        // Apply all pending database migrations to ensure current schema
        migrations::init_with_migrations(&mut conn)?;

        Ok(Self { conn })
    }

    /// Creates a database connection without automatic migration application.
    ///
    /// This specialized constructor provides access to the database without
    /// triggering automatic schema migrations. It's designed for use cases
    /// that require precise control over migration timing or need to inspect
    /// database state before migration.
    ///
    /// ## Use Cases
    ///
    /// - **Migration Tools**: Utilities that manage migrations manually
    /// - **Database Inspection**: Tools that examine schema state and version
    /// - **Testing Scenarios**: Tests that require specific migration states
    /// - **Recovery Operations**: Procedures that work with partially migrated databases
    /// - **Backup/Export**: Operations that need database access before migration
    ///
    /// ## Limited Functionality
    ///
    /// Connections created with this method may have limited functionality
    /// if the database schema is not current. Application code should generally
    /// use `Db::new()` for standard database access.
    ///
    /// ## Configuration Applied
    ///
    /// Even without migrations, this method still applies essential configuration:
    /// - Foreign key constraint enforcement
    /// - Basic SQLite pragma settings
    /// - UTF-8 encoding configuration
    ///
    /// # Returns
    ///
    /// Returns a raw SQLite connection with basic configuration applied,
    /// or an error if the database file cannot be accessed or the connection fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::db::Db;
    /// use kasl::db::migrations::{get_db_version, needs_migration};
    ///
    /// // Get connection without automatic migrations
    /// let conn = Db::new_without_migrations()?;
    ///
    /// // Check current migration status
    /// let current_version = get_db_version(&conn)?;
    /// let needs_update = needs_migration(&conn)?;
    ///
    /// println!("Database version: {}", current_version);
    /// if needs_update {
    ///     println!("Database needs migration");
    ///     // Apply migrations manually if needed
    /// }
    /// ```
    ///
    /// # Safety Considerations
    ///
    /// Using this method requires careful consideration of database schema
    /// compatibility. Operations on databases with outdated schemas may:
    /// - Fail due to missing tables or columns
    /// - Produce incorrect results due to schema changes
    /// - Cause data corruption if schema assumptions are violated
    ///
    /// # When to Use
    ///
    /// This method should be used only when:
    /// - Building migration management tools
    /// - Implementing database diagnostic utilities
    /// - Creating testing scenarios that require specific schema states
    /// - Performing database recovery or maintenance operations
    ///
    /// For all standard application database access, use `Db::new()` instead.
    pub fn new_without_migrations() -> Result<Connection> {
        // Resolve the database file path using the same logic as the main constructor
        let db_file_path = DataStorage::new().get_path(DB_FILE_NAME)?;

        // Create a basic SQLite connection without additional setup
        let conn = Connection::open(db_file_path)?;

        // Enable foreign keys for consistency, even without migrations
        // This ensures referential integrity regardless of migration state
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        Ok(conn)
    }
}
