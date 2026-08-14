//! Database connection bootstrap: path resolution, pragmas, migrations.
//!
//! ```rust,no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use kasl::db::db::Db;
//!
//! let db = Db::new()?;
//! let count: i32 = db.conn.query_row(
//!     "SELECT COUNT(*) FROM tasks",
//!     [],
//!     |row| row.get(0)
//! )?;
//! # Ok(())
//! # }
//! ```

use crate::db::migrations;
use crate::libs::data_storage::DataStorage;
use anyhow::Result;
use rusqlite::Connection;

/// Filename of the SQLite database inside the app data directory.
pub const DB_FILE_NAME: &str = "kasl.db";

/// An open, migrated SQLite connection.
///
/// SQLite connections are not thread-safe; keep each instance on one thread
/// or wrap it in a mutex (as the specialized `db::*` modules do).
pub struct Db {
    /// Connection with foreign keys on and all migrations applied.
    pub conn: Connection,
}

impl Db {
    /// Opens the database, enabling foreign keys and applying any pending
    /// migrations - the standard way to get a connection.
    ///
    /// ```rust,no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use kasl::db::db::Db;
    ///
    /// let db = Db::new()?;
    ///
    /// let task_count: i32 = db.conn.query_row(
    ///     "SELECT COUNT(*) FROM tasks",
    ///     [],
    ///     |row| row.get(0)
    /// )?;
    ///
    /// println!("Database contains {} tasks", task_count);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self> {
        let db_file_path = DataStorage::new().get_path(DB_FILE_NAME)?;
        let mut conn = Connection::open(db_file_path)?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        migrations::init_with_migrations(&mut conn)?;
        Ok(Self { conn })
    }

    /// Opens the database WITHOUT applying migrations - for migration
    /// tooling and diagnostics that must see the schema as it is. Anything
    /// else wants [`Db::new`], because queries against an outdated schema
    /// fail or lie.
    ///
    /// ```rust,no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use kasl::db::db::Db;
    /// use kasl::db::migrations::{get_db_version, needs_migration};
    ///
    /// let conn = Db::new_without_migrations()?;
    ///
    /// let current_version = get_db_version(&conn)?;
    /// let needs_update = needs_migration(&conn)?;
    ///
    /// println!("Database version: {}", current_version);
    /// if needs_update {
    ///     println!("Database needs migration");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_without_migrations() -> Result<Connection> {
        let db_file_path = DataStorage::new().get_path(DB_FILE_NAME)?;
        let conn = Connection::open(db_file_path)?;
        // Foreign keys stay on even here, so ad-hoc queries cannot break
        // referential integrity.
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        Ok(conn)
    }
}
