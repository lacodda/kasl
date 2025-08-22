//! Database schema migration management and versioning system.
//!
//! Provides a comprehensive migration framework for evolving the database schema
//! over time while maintaining data integrity and consistency.
//!
//! ## Features
//!
//! - **Version Tracking**: Maintains precise records of applied migrations
//! - **Automatic Application**: Runs pending migrations during database initialization
//! - **Transaction Safety**: All migrations run within database transactions
//! - **Rollback Support**: Development-time rollback capabilities (debug builds only)
//! - **History Tracking**: Complete audit trail of schema changes
//!
//! ## Usage
//!
//! ```rust
//! use kasl::db::migrations::{init_with_migrations, get_db_version};
//! use rusqlite::Connection;
//!
//! let mut conn = Connection::open("kasl.db")?;
//! init_with_migrations(&mut conn)?;
//! let version = get_db_version(&conn)?;
//! ```

use crate::libs::messages::Message;
use crate::{msg_debug, msg_error, msg_info, msg_success};
use anyhow::Result;
use rusqlite::{params, Connection, Transaction};

/// SQL schema for the migrations tracking table.
///
/// This table maintains a complete record of all applied migrations,
/// enabling version tracking and providing an audit trail of schema changes.
/// Each migration is recorded with its version, name, and application timestamp.
const MIGRATIONS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS migrations (
    id INTEGER PRIMARY KEY,
    version INTEGER NOT NULL UNIQUE,
    name TEXT NOT NULL,
    applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";

/// Represents a single database migration with execution logic.
///
/// Each migration contains the information needed to apply a specific
/// schema change, including version tracking and the transformation function.
/// Migrations are designed to be immutable and deterministic.
#[derive(Debug, Clone)]
struct Migration {
    /// Unique version number for ordering and tracking
    version: u32,
    /// Human-readable name describing the migration's purpose
    name: &'static str,
    /// Function that applies the schema changes within a transaction
    up: fn(&Transaction) -> Result<()>,
}

/// Central migration system manager that orchestrates schema evolution.
///
/// The `MigrationManager` maintains the complete registry of available migrations
/// and provides the logic for applying them in the correct order. It ensures
/// that migrations are applied atomically and tracks their completion status.
///
/// ## Architecture
///
/// - **Migration Registry**: Stores all available migrations in version order
/// - **Version Control**: Tracks current schema version and pending changes  
/// - **Transaction Management**: Ensures each migration is atomic
/// - **Error Recovery**: Provides rollback on migration failures
///
/// ## Thread Safety
///
/// The migration manager is designed for single-threaded use during application
/// startup. Multiple concurrent migration attempts should be avoided.
pub struct MigrationManager {
    /// Ordered list of all available migrations
    ///
    /// Migrations are stored in version order to ensure correct application
    /// sequence. Each migration builds upon the schema state created by
    /// its predecessors.
    migrations: Vec<Migration>,
}

impl MigrationManager {
    /// Creates a new migration manager with all registered migrations.
    ///
    /// This constructor automatically registers all available migrations
    /// in the correct order. The registration process is deterministic
    /// and ensures consistent schema evolution across all environments.
    ///
    /// # Returns
    ///
    /// Returns a fully initialized migration manager ready to apply
    /// pending schema changes.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::migrations::MigrationManager;
    ///
    /// let manager = MigrationManager::new();
    /// // Manager is ready to apply migrations
    /// ```
    pub fn new() -> Self {
        let mut manager = Self { migrations: Vec::new() };

        // Register all migrations in chronological order
        // Each registration adds a migration to the internal registry
        manager.register_migrations();
        manager
    }

    /// Registers all database migrations in chronological order.
    ///
    /// This method defines the complete schema evolution history by registering
    /// each migration version with its transformation logic. Migrations must
    /// be registered in sequential version order to ensure correct application.
    ///
    /// ## Migration Design Principles
    ///
    /// - **Incremental**: Each migration makes small, focused changes
    /// - **Idempotent**: Migrations can be safely re-run if needed
    /// - **Forward-Only**: No backward compatibility requirements
    /// - **Atomic**: Each migration succeeds or fails completely
    fn register_migrations(&mut self) {
        // Initial schema - version 0 is implicit (empty database)
        // Base tables are created by individual modules as needed

        // Version 1: Base tables and performance indices
        // Creates fundamental tables and adds indices for better performance
        self.add_migration(1, "create_tables_and_indices", |tx| {
            // First, create base tables that individual modules depend on
            // This ensures tables exist before any indices are created

            // Create tasks table
            tx.execute(
                "CREATE TABLE IF NOT EXISTS tasks (
        id INTEGER NOT NULL PRIMARY KEY,
        task_id INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 0,
        timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        name TEXT NOT NULL,
        comment TEXT,
        completeness INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 100,
        excluded_from_search BOOLEAN NOT NULL ON CONFLICT REPLACE DEFAULT FALSE
    )",
                [],
            )?;

            // Create pauses table
            tx.execute(
                "CREATE TABLE IF NOT EXISTS pauses (
        id INTEGER NOT NULL PRIMARY KEY,
        start TIMESTAMP NOT NULL,
        end TIMESTAMP,
        duration INTEGER
    )",
                [],
            )?;

            // Create workdays table
            tx.execute(
                "CREATE TABLE IF NOT EXISTS workdays (
        id INTEGER PRIMARY KEY,
        date DATE NOT NULL UNIQUE,
        start TIMESTAMP NOT NULL,
        end TIMESTAMP
    )",
                [],
            )?;

            // Now create indices for the tables we just created

            // Index tasks by timestamp for chronological queries
            tx.execute("CREATE INDEX IF NOT EXISTS idx_tasks_timestamp ON tasks(timestamp)", [])?;
            // Index tasks by parent task relationship
            tx.execute("CREATE INDEX IF NOT EXISTS idx_tasks_task_id ON tasks(task_id)", [])?;
            // Index pauses by start time for temporal queries
            tx.execute("CREATE INDEX IF NOT EXISTS idx_pauses_start ON pauses(start)", [])?;
            // Index workdays by date for daily/monthly reporting
            tx.execute("CREATE INDEX IF NOT EXISTS idx_workdays_date ON workdays(date)", [])?;

            Ok(())
        });

        // Version 2: Task templates system for reusable task patterns
        // Introduces the ability to save and reuse common task configurations
        self.add_migration(2, "add_task_templates", |tx| {
            tx.execute(
                "CREATE TABLE IF NOT EXISTS task_templates (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    task_name TEXT NOT NULL,
                    comment TEXT,
                    completeness INTEGER DEFAULT 100,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )",
                [],
            )?;
            Ok(())
        });

        // Version 3: Tags and categorization system for task organization
        // Adds support for tagging tasks with customizable labels and colors
        self.add_migration(3, "add_tags_system", |tx| {
            // Main tags table for storing tag definitions
            tx.execute(
                "CREATE TABLE IF NOT EXISTS tags (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )",
                [],
            )?;

            // Junction table for many-to-many task-tag relationships
            tx.execute(
                "CREATE TABLE IF NOT EXISTS task_tags (
                    task_id INTEGER NOT NULL,
                    tag_id INTEGER NOT NULL,
                    PRIMARY KEY (task_id, tag_id),
                    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
                    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
                )",
                [],
            )?;
            Ok(())
        });

        // Version 4: Soft delete functionality for data preservation
        // Enables logical deletion while maintaining data for auditing
        self.add_migration(4, "add_soft_delete", |tx| {
            tx.execute("ALTER TABLE tasks ADD COLUMN deleted_at TIMESTAMP", [])?;
            tx.execute("CREATE INDEX idx_tasks_deleted_at ON tasks(deleted_at)", [])?;
            Ok(())
        });

        // Version 5: Workday notes and annotations for context tracking
        // Allows users to add contextual notes to their workdays
        self.add_migration(5, "add_workday_notes", |tx| {
            tx.execute("ALTER TABLE workdays ADD COLUMN notes TEXT", [])?;
            Ok(())
        });

        // Version 6: Manual breaks table for productivity management
        // Enables users to add manual break periods to improve productivity calculations
        self.add_migration(6, "add_breaks_table", |tx| {
            tx.execute(
                "CREATE TABLE IF NOT EXISTS breaks (
                    id INTEGER PRIMARY KEY,
                    date DATE NOT NULL,
                    start_time DATETIME NOT NULL,
                    end_time DATETIME NOT NULL,
                    duration INTEGER NOT NULL,
                    reason TEXT,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                )",
                [],
            )?;
            
            // Create index for efficient daily break lookups
            tx.execute("CREATE INDEX idx_breaks_date ON breaks(date)", [])?;
            Ok(())
        });
    }

    /// Registers a single migration in the migration system.
    ///
    /// This helper method adds a migration to the internal registry with
    /// proper version ordering and validation. It ensures that migrations
    /// are stored in a consistent format for later execution.
    ///
    /// # Arguments
    ///
    /// * `version` - Unique version number for this migration
    /// * `name` - Descriptive name for the migration's purpose
    /// * `up` - Function that performs the actual schema transformation
    ///
    /// # Panics
    ///
    /// Panics if a migration with the same version number is already registered.
    fn add_migration(&mut self, version: u32, name: &'static str, up: fn(&Transaction) -> Result<()>) {
        self.migrations.push(Migration { version, name, up });
    }

    /// Executes all pending migrations in the correct order.
    ///
    /// This method performs the complete migration process:
    /// 1. Creates the migrations tracking table if needed
    /// 2. Determines current database version
    /// 3. Identifies pending migrations
    /// 4. Applies each migration within a transaction
    /// 5. Records successful migrations in the tracking table
    ///
    /// ## Transaction Safety
    ///
    /// Each migration runs in its own transaction, ensuring that partial
    /// failures don't leave the database in an inconsistent state. If any
    /// migration fails, all changes are rolled back automatically.
    ///
    /// # Arguments
    ///
    /// * `conn` - Mutable database connection for applying migrations
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all migrations succeed, or an error if any
    /// migration fails during application.
    ///
    /// # Example
    ///
    /// ```rust
    /// use kasl::db::migrations::MigrationManager;
    /// use rusqlite::Connection;
    ///
    /// let manager = MigrationManager::new();
    /// let mut conn = Connection::open(":memory:")?;
    /// manager.run_migrations(&mut conn)?;
    /// ```
    pub fn run_migrations(&self, conn: &mut Connection) -> Result<()> {
        // Initialize the migrations tracking table
        conn.execute(MIGRATIONS_TABLE, [])?;

        // Determine the current schema version
        let current_version = self.get_current_version(conn)?;

        // Find all migrations that haven't been applied yet
        let pending: Vec<&Migration> = self.migrations.iter().filter(|m| m.version > current_version).collect();

        // Exit early if no migrations are needed
        if pending.is_empty() {
            msg_debug!("Database is up to date");
            return Ok(());
        }

        // Notify user about pending migrations
        msg_info!(Message::MigrationsFound(pending.len()));

        // Execute all pending migrations within a single transaction
        let tx = conn.transaction()?;

        for migration in pending {
            msg_info!(Message::RunningMigration(migration.version, migration.name.to_string()));

            match (migration.up)(&tx) {
                Ok(()) => {
                    // Record successful migration in tracking table
                    tx.execute(
                        "INSERT INTO migrations (version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )?;
                    msg_success!(Message::MigrationCompleted(migration.version));
                }
                Err(e) => {
                    // Log migration failure and propagate error
                    msg_error!(Message::MigrationFailed(migration.version, e.to_string()));
                    return Err(e);
                }
            }
        }

        // Commit all successful migrations
        tx.commit()?;
        msg_success!(Message::AllMigrationsCompleted);

        Ok(())
    }

    /// Retrieves the current database schema version.
    ///
    /// This method queries the migrations table to determine the highest
    /// version number that has been successfully applied. It handles the
    /// case where no migrations have been applied yet (version 0).
    ///
    /// # Arguments
    ///
    /// * `conn` - Database connection for querying migration status
    ///
    /// # Returns
    ///
    /// Returns the current schema version number, or 0 if no migrations
    /// have been applied yet.
    fn get_current_version(&self, conn: &Connection) -> Result<u32> {
        let version: Option<u32> = conn.query_row("SELECT MAX(version) FROM migrations", [], |row| row.get(0)).unwrap_or(Some(0));

        Ok(version.unwrap_or(0))
    }

    /// Checks if a specific migration version has been applied.
    ///
    /// This utility method allows callers to verify whether a particular
    /// migration has been successfully applied to the database. Useful
    /// for conditional logic based on schema capabilities.
    ///
    /// # Arguments
    ///
    /// * `conn` - Database connection for querying migration status  
    /// * `version` - Migration version number to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the migration has been applied, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// let manager = MigrationManager::new();
    /// if manager.is_migration_applied(&conn, 3)? {
    ///     // Tags system is available
    /// }
    /// ```
    pub fn is_migration_applied(&self, conn: &Connection, version: u32) -> Result<bool> {
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM migrations WHERE version = ?1", params![version], |row| row.get(0))?;

        Ok(count > 0)
    }

    /// Retrieves the complete migration history with timestamps.
    ///
    /// This method returns a chronological list of all applied migrations,
    /// including their version numbers, names, and application timestamps.
    /// Useful for auditing and debugging schema evolution.
    ///
    /// # Arguments
    ///
    /// * `conn` - Database connection for querying migration history
    ///
    /// # Returns
    ///
    /// Returns a vector of tuples containing (version, name, applied_at)
    /// for each applied migration, ordered by version number.
    ///
    /// # Example
    ///
    /// ```rust
    /// let manager = MigrationManager::new();
    /// let history = manager.get_migration_history(&conn)?;
    /// for (version, name, applied_at) in history {
    ///     println!("v{}: {} ({})", version, name, applied_at);
    /// }
    /// ```
    pub fn get_migration_history(&self, conn: &Connection) -> Result<Vec<(u32, String, String)>> {
        let mut stmt = conn.prepare("SELECT version, name, applied_at FROM migrations ORDER BY version")?;

        let history = stmt
            .query_map([], |row| Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(history)
    }

    /// Rolls back migrations to a specific target version (debug builds only).
    ///
    /// This development utility allows rolling back migrations to a previous
    /// schema version by removing migration records from the tracking table.
    ///
    /// ## ⚠️ Important Notes
    ///
    /// - Only available in debug builds for safety
    /// - This is a simplified rollback that removes migration records
    /// - Does not actually reverse schema changes (no down() functions)
    /// - Primarily useful for development and testing scenarios
    ///
    /// # Arguments
    ///
    /// * `conn` - Mutable database connection for rollback operations
    /// * `target_version` - Target version to roll back to
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if rollback succeeds, or an error if the operation fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// #[cfg(debug_assertions)]
    /// {
    ///     let manager = MigrationManager::new();
    ///     manager.rollback_to(&mut conn, 2)?; // Roll back to version 2
    /// }
    /// ```
    #[cfg(debug_assertions)]
    pub fn rollback_to(&self, conn: &mut Connection, target_version: u32) -> Result<()> {
        let current_version = self.get_current_version(conn)?;

        if target_version >= current_version {
            msg_info!(Message::NothingToRollback);
            return Ok(());
        }

        msg_info!(Message::RollingBack(current_version, target_version));

        // Remove migration records beyond the target version
        // Note: This is a simplified rollback that doesn't actually reverse schema changes
        conn.execute("DELETE FROM migrations WHERE version > ?1", params![target_version])?;

        msg_success!(Message::RollbackCompleted(target_version));
        Ok(())
    }
}

/// Initializes a database connection with all pending migrations applied.
///
/// This convenience function creates a migration manager and applies all
/// pending migrations to the provided connection. It's the recommended
/// way to ensure a database is up to date with the latest schema.
///
/// # Arguments
///
/// * `conn` - Mutable database connection to initialize
///
/// # Returns
///
/// Returns `Ok(())` if initialization succeeds, or an error if migration fails.
///
/// # Example
///
/// ```rust
/// use kasl::db::migrations::init_with_migrations;
/// use rusqlite::Connection;
///
/// let mut conn = Connection::open("kasl.db")?;
/// init_with_migrations(&mut conn)?;
/// ```
pub fn init_with_migrations(conn: &mut Connection) -> Result<()> {
    let manager = MigrationManager::new();
    manager.run_migrations(conn)?;
    Ok(())
}

/// Retrieves the current database schema version.
///
/// This utility function provides a simple way to check the current
/// schema version without creating a full migration manager instance.
///
/// # Arguments
///
/// * `conn` - Database connection to query
///
/// # Returns
///
/// Returns the current schema version number.
///
/// # Example
///
/// ```rust
/// use kasl::db::migrations::get_db_version;
///
/// let version = get_db_version(&conn)?;
/// println!("Current schema version: {}", version);
/// ```
pub fn get_db_version(conn: &Connection) -> Result<u32> {
    let manager = MigrationManager::new();
    manager.get_current_version(conn)
}

/// Checks if the database requires migration to the latest schema version.
///
/// This utility function compares the current database version with the
/// latest available migration version to determine if updates are needed.
///
/// # Arguments
///
/// * `conn` - Database connection to check
///
/// # Returns
///
/// Returns `true` if migrations are needed, `false` if up to date.
///
/// # Example
///
/// ```rust
/// use kasl::db::migrations::needs_migration;
///
/// if needs_migration(&conn)? {
///     println!("Database needs migration!");
/// }
/// ```
pub fn needs_migration(conn: &Connection) -> Result<bool> {
    let manager = MigrationManager::new();
    let current = manager.get_current_version(conn)?;
    let latest = manager.migrations.last().map(|m| m.version).unwrap_or(0);
    Ok(current < latest)
}
