use crate::libs::messages::Message;
use crate::{msg_debug, msg_error, msg_info, msg_success};
use anyhow::Result;
use rusqlite::{params, Connection, Transaction};

const MIGRATIONS_TABLE: &str = "
CREATE TABLE IF NOT EXISTS migrations (
    id INTEGER PRIMARY KEY,
    version INTEGER NOT NULL UNIQUE,
    name TEXT NOT NULL,
    applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)";

/// Represents a single database migration
#[derive(Debug, Clone)]
struct Migration {
    version: u32,
    name: &'static str,
    up: fn(&Transaction) -> Result<()>,
}

/// The migration system manager
pub struct MigrationManager {
    migrations: Vec<Migration>,
}

impl MigrationManager {
    pub fn new() -> Self {
        let mut manager = Self { migrations: Vec::new() };

        // Register all migrations here
        manager.register_migrations();
        manager
    }

    /// Register all database migrations in order
    fn register_migrations(&mut self) {
        // Initial schema - version 0 is implicit (empty database)

        // Version 1: Add indices for better performance
        self.add_migration(1, "add_indices", |tx| {
            tx.execute("CREATE INDEX IF NOT EXISTS idx_tasks_timestamp ON tasks(timestamp)", [])?;
            tx.execute("CREATE INDEX IF NOT EXISTS idx_tasks_task_id ON tasks(task_id)", [])?;
            tx.execute("CREATE INDEX IF NOT EXISTS idx_pauses_start ON pauses(start)", [])?;
            tx.execute("CREATE INDEX IF NOT EXISTS idx_workdays_date ON workdays(date)", [])?;
            Ok(())
        });

        // Version 2: Add task templates table
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

        // Version 3: Add tags system
        self.add_migration(3, "add_tags_system", |tx| {
            // Tags table
            tx.execute(
                "CREATE TABLE IF NOT EXISTS tags (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )",
                [],
            )?;

            // Task tags junction table
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

        // Version 4: Add soft delete to tasks
        self.add_migration(4, "add_soft_delete", |tx| {
            tx.execute("ALTER TABLE tasks ADD COLUMN deleted_at TIMESTAMP", [])?;
            tx.execute("CREATE INDEX idx_tasks_deleted_at ON tasks(deleted_at)", [])?;
            Ok(())
        });

        // Version 5: Add notes to workdays
        self.add_migration(5, "add_workday_notes", |tx| {
            tx.execute("ALTER TABLE workdays ADD COLUMN notes TEXT", [])?;
            Ok(())
        });
    }

    /// Add a migration to the registry
    fn add_migration(&mut self, version: u32, name: &'static str, up: fn(&Transaction) -> Result<()>) {
        self.migrations.push(Migration { version, name, up });
    }

    /// Run all pending migrations
    pub fn run_migrations(&self, conn: &mut Connection) -> Result<()> {
        // Create migrations table if it doesn't exist
        conn.execute(MIGRATIONS_TABLE, [])?;

        // Get current version
        let current_version = self.get_current_version(conn)?;

        // Find pending migrations
        let pending: Vec<&Migration> = self.migrations.iter().filter(|m| m.version > current_version).collect();

        if pending.is_empty() {
            msg_debug!("Database is up to date");
            return Ok(());
        }

        msg_info!(Message::MigrationsFound(pending.len()));

        // Run migrations in a transaction
        let tx = conn.transaction()?;

        for migration in pending {
            msg_info!(Message::RunningMigration(migration.version, migration.name.to_string()));

            match (migration.up)(&tx) {
                Ok(()) => {
                    // Record successful migration
                    tx.execute(
                        "INSERT INTO migrations (version, name) VALUES (?1, ?2)",
                        params![migration.version, migration.name],
                    )?;
                    msg_success!(Message::MigrationCompleted(migration.version));
                }
                Err(e) => {
                    msg_error!(Message::MigrationFailed(migration.version, e.to_string()));
                    return Err(e);
                }
            }
        }

        tx.commit()?;
        msg_success!(Message::AllMigrationsCompleted);

        Ok(())
    }

    /// Get the current database version
    fn get_current_version(&self, conn: &Connection) -> Result<u32> {
        let version: Option<u32> = conn.query_row("SELECT MAX(version) FROM migrations", [], |row| row.get(0)).unwrap_or(Some(0));

        Ok(version.unwrap_or(0))
    }

    /// Check if a specific migration has been applied
    pub fn is_migration_applied(&self, conn: &Connection, version: u32) -> Result<bool> {
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM migrations WHERE version = ?1", params![version], |row| row.get(0))?;

        Ok(count > 0)
    }

    /// Get migration history
    pub fn get_migration_history(&self, conn: &Connection) -> Result<Vec<(u32, String, String)>> {
        let mut stmt = conn.prepare("SELECT version, name, applied_at FROM migrations ORDER BY version")?;

        let history = stmt
            .query_map([], |row| Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(history)
    }

    /// Rollback to a specific version (for development/debugging)
    #[cfg(debug_assertions)]
    pub fn rollback_to(&self, conn: &mut Connection, target_version: u32) -> Result<()> {
        let current_version = self.get_current_version(conn)?;

        if target_version >= current_version {
            msg_info!(Message::NothingToRollback);
            return Ok(());
        }

        msg_info!(Message::RollingBack(current_version, target_version));

        // This is a simplified rollback that just removes migration records
        // In a real system, you'd want down() functions for each migration
        conn.execute("DELETE FROM migrations WHERE version > ?1", params![target_version])?;

        msg_success!(Message::RollbackCompleted(target_version));
        Ok(())
    }
}

/// Initialize database with migrations
pub fn init_with_migrations(conn: &mut Connection) -> Result<()> {
    let manager = MigrationManager::new();
    manager.run_migrations(conn)?;
    Ok(())
}

/// Get current database version
pub fn get_db_version(conn: &Connection) -> Result<u32> {
    let manager = MigrationManager::new();
    manager.get_current_version(conn)
}

/// Check if database needs migration
pub fn needs_migration(conn: &Connection) -> Result<bool> {
    let manager = MigrationManager::new();
    let current = manager.get_current_version(conn)?;
    let latest = manager.migrations.last().map(|m| m.version).unwrap_or(0);
    Ok(current < latest)
}
