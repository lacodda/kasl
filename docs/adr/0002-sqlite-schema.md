# ADR 0002: SQLite with versioned migrations

- Status: accepted
- Date: 2026-08-08

## Context

kasl is a single-user desktop tool: one person, one machine, no server to run and no concurrent writers to coordinate with. It needs to persist workdays, pauses, tasks, tags, templates and a Jira inbox between runs of a CLI that is invoked constantly (every command opens a connection) and by a background daemon that opens its own.

The schema has changed repeatedly since the first release - tags, templates, soft delete, notes, a Jira inbox, and a table that was added and later folded back into another. Each of these had to reach every existing install without a migration step the user runs by hand, and without losing rows already on disk.

## Decision

Data lives in a single SQLite file, `kasl.db`, in the OS-specific per-user data directory (`DataStorage`, e.g. `%LOCALAPPDATA%\lacodda\kasl\kasl.db` on Windows). No server, no connection pool - `rusqlite::Connection::open` with `PRAGMA foreign_keys = ON`.

Schema evolution goes through a hand-rolled migration framework (`src/db/migrations.rs`), not an external migration tool. Each migration is a `(version: u32, name: &str, up: fn(&Transaction) -> Result<()>)` triple registered in order by `MigrationManager::register_migrations`. A `migrations` table (`version` unique, `name`, `applied_at`) records what has been applied. On `Db::new()`, `init_with_migrations` runs automatically: it creates the `migrations` table if absent, diffs the registered versions against `MAX(version)` already recorded, and - if any are pending - applies all of them inside one transaction, inserting a `migrations` row per successful step before committing. A failure mid-migration rolls back the whole batch; nothing partial is left applied. There are 11 migrations as of this writing, from `1: create_tables_and_indices` (base `tasks`/`pauses`/`workdays` tables and their indices) through `11: fold_breaks_into_protected_pauses` (which drops the `breaks` table after copying its rows into `pauses` with a new `protected` column).

`Db::new_without_migrations()` opens a connection without running the migration step, used by the debug-only `kasl migrations` command (`status`, `history`) to inspect version and history without side effects. `MigrationManager::rollback_to` exists but is compiled only under `#[cfg(debug_assertions)]`, is not wired to any CLI subcommand, and does not reverse schema changes - it only deletes rows from `migrations` above the target version, so a "rolled back" database still has the columns and tables the later migrations created.

## Consequences

- A migration, once released, is immutable: fixing a mistake means writing a new migration with the next version number, never editing an old `up` function - a connection that already recorded the old version will never re-run it.
- Individual table modules (`workdays.rs`, `pauses.rs`, etc.) still carry their own `CREATE TABLE IF NOT EXISTS` schema constants and run them in their own constructors, so a table's schema is described in two places: the module's own `SCHEMA_*` constant (which must independently stay in sync with the latest migration) and the migration history that produced it.
- There is no real rollback outside debug builds, and even in debug builds rollback only erases bookkeeping, not columns - recovering a pre-migration schema means restoring a database backup.
- Every `up` function runs inside a single shared transaction with all other pending migrations for that startup; a `DROP TABLE`/data-copy migration like version 11 must get its statement order right the first time, since there is no per-migration retry.
- Startup cost grows, in principle, with migration count, since `run_migrations` always builds the full registry and diffs it against the stored version - negligible at 11 migrations, but the list only grows.
