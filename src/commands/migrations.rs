//! Database migration management command (debug builds only).
//!
//! Provides database schema management utilities for development and debugging purposes.
//!
//! ## Features
//!
//! - **Version Tracking**: Maintains current database schema version
//! - **Migration History**: Records all applied migrations with timestamps
//! - **Debug Only**: Available only in debug builds for production safety
//! - **Status Inspection**: View current migration status and history
//! - **Integrity Checking**: Validates database schema consistency
//!
//! ## Usage
//!
//! ```bash
//! # Check migration status (debug builds only)
//! kasl migrations status
//!
//! # View migration history (debug builds only)
//! kasl migrations history
//! ```

#[cfg(debug_assertions)]
use crate::{
    db::{
        db::Db,
        migrations::{get_db_version, needs_migration, MigrationManager},
    },
    libs::messages::Message,
    msg_info, msg_print,
};
#[cfg(debug_assertions)]
use anyhow::Result;
#[cfg(debug_assertions)]
use clap::{Args, Subcommand};

/// Command-line arguments for database migration management.
///
/// This command provides essential tools for database schema inspection
/// and management during development. All operations are read-only or
/// carefully controlled to prevent accidental data loss.
#[cfg(debug_assertions)]
#[derive(Debug, Args)]
pub struct MigrationsArgs {
    #[command(subcommand)]
    command: MigrationsCommand,
}

/// Available migration management operations.
///
/// Each subcommand provides specific functionality for database schema
/// inspection and management. Operations are designed to be safe and
/// informative for development workflows.
#[cfg(debug_assertions)]
#[derive(Debug, Subcommand)]
enum MigrationsCommand {
    /// Display current database schema version and migration status
    ///
    /// Shows the current database version and indicates whether any
    /// pending migrations need to be applied. This is useful for
    /// understanding the current state of the database schema during
    /// development and troubleshooting.
    Status,

    /// Show complete migration history with timestamps
    ///
    /// Displays a chronological list of all migrations that have been
    /// applied to the database, including version numbers, migration
    /// names, and application timestamps. Useful for understanding
    /// how the database schema has evolved over time.
    History,
}

/// Executes database migration management operations.
///
/// Provides essential database schema inspection capabilities for development
/// and debugging. All operations are designed to be safe and non-destructive.
///
/// # Arguments
///
/// * `args` - Parsed command-line arguments specifying the inspection operation
///
/// # Returns
///
/// Returns `Ok(())` on successful operation completion, or an error if
/// database access fails or the requested operation encounters issues.
///
/// # Examples
///
/// ```bash
/// # Check current database version and migration status
/// kasl migrations status
///
/// # View complete migration history
/// kasl migrations history
/// ```
///
/// # Error Scenarios
///
/// - Database connection failures
/// - Corrupted migration tracking tables
/// - Inconsistent schema state
/// - Permission issues accessing database files
#[cfg(debug_assertions)]
pub fn cmd(args: MigrationsArgs) -> Result<()> {
    // Create direct database connection without running migrations
    // This ensures we can inspect the current state without modifying it
    let conn = Db::new_without_migrations()?;

    match args.command {
        MigrationsCommand::Status => {
            // Get current database version from migration tracking table
            let version = get_db_version(&conn)?;

            // Check if any migrations are pending application
            let needs_update = needs_migration(&conn)?;

            // Display current version information
            msg_print!(Message::DatabaseVersion(version));

            // Provide clear status about migration needs
            if needs_update {
                msg_info!(Message::DatabaseNeedsUpdate);
            } else {
                msg_info!(Message::DatabaseUpToDate);
            }
        }
        MigrationsCommand::History => {
            // Create migration manager for history access
            let manager = MigrationManager::new();

            // Retrieve complete migration history from database
            let history = manager.get_migration_history(&conn)?;

            // Display formatted migration history
            msg_print!(Message::MigrationHistory, true);
            for (version, name, applied_at) in history {
                println!("  v{}: {} (applied: {})", version, name, applied_at);
            }
        }
    }

    Ok(())
}
