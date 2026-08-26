---
title: "Database"
---

kasl uses SQLite as its local database for storing work sessions, tasks, and configuration data.

## Overview

The database provides:
- **Local Storage**: All data stored locally for privacy
- **Migration System**: Schema changes are applied automatically, in order, on every startup
- **Cross-Platform**: Works on all supported platforms

## Database Location

Database files are stored in platform-specific locations:

- **Windows**: `%LOCALAPPDATA%\lacodda\kasl\kasl.db`
- **macOS**: `~/Library/Application Support/lacodda/kasl/kasl.db`
- **Linux**: `~/.local/share/lacodda/kasl/kasl.db`

On every open, kasl runs `PRAGMA foreign_keys = ON` and applies any pending migrations.

## Schema Overview

### Tables

#### `workdays`
Stores daily work session information:
```sql
CREATE TABLE workdays (
    id INTEGER PRIMARY KEY,
    date DATE NOT NULL UNIQUE,
    start TIMESTAMP NOT NULL,
    end TIMESTAMP,
    notes TEXT
);
```

#### `pauses`
Stores break periods during work sessions:
```sql
CREATE TABLE pauses (
    id INTEGER NOT NULL PRIMARY KEY,
    start TIMESTAMP NOT NULL,
    end TIMESTAMP,
    duration INTEGER,
    protected INTEGER NOT NULL DEFAULT 0,
    reason TEXT
);
```

`protected` marks a pause entered by hand with `kasl pauses add --keep`. Protected pauses are exempt from both cleanup filters applied to detected pauses: the minimum-duration threshold and merging with an adjacent pause. `reason` is the optional note passed via `--reason`. See [`pauses`](/reference/pauses/) for the filtering and productivity rules.

#### `tasks`
Stores task information and metadata:
```sql
CREATE TABLE tasks (
    id INTEGER NOT NULL PRIMARY KEY,
    task_id INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 0,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER NOT NULL ON CONFLICT REPLACE DEFAULT 100,
    excluded_from_search BOOLEAN NOT NULL ON CONFLICT REPLACE DEFAULT FALSE,
    deleted_at TIMESTAMP,
    jira_key TEXT
);
```

- `task_id` groups a task with its own history across days - it points at the id of the first task in the chain, so `task find` can offer yesterday's unfinished work and see today's progress as the same item. It is not an external reference.
- `jira_key` is the Jira issue the task was taken from, set by `kasl inbox take`. The `UPDATE` behind `kasl task edit` does not list this column, so renaming a task cannot detach it from its issue.
- `deleted_at` was added for soft delete, but nothing in the current codebase sets or reads it - `kasl task remove` deletes rows outright. Treat the column as reserved.

#### `tags`
Stores task categorization tags:
```sql
CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

#### `task_tags`
Links tasks to tags (many-to-many relationship):
```sql
CREATE TABLE task_tags (
    task_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    PRIMARY KEY (task_id, tag_id),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);
```

#### `task_templates`
Stores reusable task templates:
```sql
CREATE TABLE task_templates (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    task_name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER DEFAULT 100,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

#### `jira_inbox`
Stores Jira issues assigned to you, synced by the background watcher. See [`inbox`](/reference/inbox/) for the command that reads and manages this table:
```sql
CREATE TABLE jira_inbox (
    issue_key TEXT PRIMARY KEY NOT NULL,
    issue_id TEXT NOT NULL,
    summary TEXT NOT NULL,
    priority TEXT,
    priority_rank INTEGER NOT NULL DEFAULT 999,
    url TEXT NOT NULL,
    first_seen TIMESTAMP NOT NULL,
    last_seen TIMESTAMP NOT NULL,
    notified INTEGER NOT NULL DEFAULT 0,
    pinned INTEGER NOT NULL DEFAULT 0,
    dismissed INTEGER NOT NULL DEFAULT 0,
    raw_updated TEXT,
    status_id TEXT,
    sort_value REAL,
    gone_at TIMESTAMP,
    last_change TEXT,
    changed_at TIMESTAMP,
    taken_at TIMESTAMP
);
```

- `status_id` references `jira_statuses.id` and resolves to a display name via join.
- `sort_value` is the numeric value of a configured Jira custom field (e.g. Scoring), used to rank issues.
- `gone_at` is stamped when an issue stops appearing in the Jira poll (closed or reassigned); it clears if the issue reappears. Rows with `gone_at` set are hidden from the default list and only shown with `kasl inbox --all`.
- `last_change` / `changed_at` record the most recent visible change (status, priority, or score) so the list can badge it.
- `taken_at` is stamped by `kasl inbox take`. Unlike `dismissed`, it keeps the row in the list: a taken issue is still part of the picture, it is just already in hand.
- `pinned` and `dismissed` are set by `kasl inbox pin` / `kasl inbox dismiss`.

#### `jira_statuses`
Local catalog of Jira workflow statuses, populated from issue sync so `jira_inbox.status_id` can resolve to a name:
```sql
CREATE TABLE jira_statuses (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);
```

#### `migrations`
Tracks database schema version:
```sql
CREATE TABLE migrations (
    id INTEGER PRIMARY KEY,
    version INTEGER NOT NULL UNIQUE,
    name TEXT NOT NULL,
    applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

## Data Types

### Timestamps
- **Format**: ISO 8601 (`YYYY-MM-DD HH:MM:SS`)
- **Timezone**: Local system time
- **Storage**: SQLite `TIMESTAMP` affinity (stored as text)

### Dates
- **Format**: ISO 8601 (`YYYY-MM-DD`)

### Durations
- **Unit**: Seconds
- **Storage**: INTEGER for efficient calculations

### Booleans
- **Storage**: INTEGER (0 = false, 1 = true)
- **SQLite standard**: No native boolean type

## Migration System

Migrations run automatically whenever kasl opens the database - there is no separate command to trigger them:

```bash
kasl watch  # opening the database applies any pending migrations
```

Schema history, in order:

1. `create_tables_and_indices` - base `tasks`, `pauses`, `workdays` tables and their indices
2. `add_task_templates` - `task_templates`
3. `add_tags_system` - `tags`, `task_tags`
4. `add_soft_delete` - `deleted_at` column and index on `tasks`
5. `add_workday_notes` - `notes` column on `workdays`
6. `add_breaks_table` - manual breaks table (later folded away, see migration 11)
7. `add_jira_inbox_table` - `jira_inbox`
8. `jira_inbox_status_id_and_sort_value` - `jira_statuses`, `status_id`/`sort_value` on `jira_inbox`
9. `clear_jira_inbox_legacy_status_text` - clears the legacy `status` text column
10. `drop_jira_inbox_legacy_status_column` - drops it
11. `fold_breaks_into_protected_pauses` - adds `protected`/`reason` to `pauses`, migrates rows out of `breaks` as protected pauses, drops `breaks`
12. `jira_inbox_gone_and_change_tracking` - adds `gone_at`, `last_change`, `changed_at` to `jira_inbox`
13. `link_taken_issues_to_their_tasks` - adds `jira_key` and its index to `tasks`, and `taken_at` to `jira_inbox`

Each migration runs inside a transaction; a failure rolls back that migration.

### Inspecting migrations

Debug builds only expose a `migrations` subcommand for inspection:

```bash
kasl migrations status   # current version, pending or up to date
kasl migrations history  # applied migrations with timestamps
```

This command does not exist in release builds - it is compiled out (`#[cfg(debug_assertions)]`). Do not point end users at it; on a release install, use direct SQL against the `migrations` table instead if you need to check the version:

```bash
sqlite3 kasl.db "SELECT * FROM migrations ORDER BY version;"
```

## Data Management

### Backup

Create database backups:
```bash
# Copy database file
cp ~/.local/share/lacodda/kasl/kasl.db kasl_backup.db

# Export data
kasl export all --format json --output backup.json
```

### Restore

Restore from backup:
```bash
# Replace database file
cp kasl_backup.db ~/.local/share/lacodda/kasl/kasl.db
```

There is no `kasl import` command. A JSON export from `kasl export all` is for reading or archiving outside kasl, not for reloading back into the database - restoring means replacing the `.db` file itself.

### Cleanup

Remove old data:
```bash
# Remove specific tasks
kasl task remove 1 2 3

# Remove all today's tasks
kasl task remove --today

# Delete old pauses (manual SQL)
sqlite3 kasl.db "DELETE FROM pauses WHERE start < date('now', '-30 days');"
```

## Indexes

```sql
-- Workdays table
CREATE INDEX idx_workdays_date ON workdays(date);

-- Tasks table
CREATE INDEX idx_tasks_timestamp ON tasks(timestamp);
CREATE INDEX idx_tasks_task_id ON tasks(task_id);
CREATE INDEX idx_tasks_deleted_at ON tasks(deleted_at);
CREATE INDEX idx_tasks_jira_key ON tasks(jira_key);

-- Pauses table
CREATE INDEX idx_pauses_start ON pauses(start);

-- Jira inbox table
CREATE INDEX idx_jira_inbox_active ON jira_inbox(dismissed, pinned DESC, priority_rank ASC, last_seen DESC);
CREATE INDEX idx_jira_inbox_sort ON jira_inbox(dismissed, pinned DESC, sort_value DESC, priority_rank ASC);
```

## Troubleshooting

### Common Issues

**Problem**: Database locked
```bash
# Check for running processes
ps aux | grep kasl

# Stop all kasl processes
kasl watch --stop

# Check file permissions
ls -la ~/.local/share/lacodda/kasl/kasl.db
```

**Problem**: Corrupted database
```bash
# Check database integrity
sqlite3 kasl.db "PRAGMA integrity_check;"

# Recover if possible
sqlite3 kasl.db ".recover" | sqlite3 kasl_recovered.db

# Restore from backup
cp kasl_backup.db kasl.db
```

### Debug Database

```bash
# Show SQL queries
RUST_LOG=kasl=debug kasl report

# Direct database access
sqlite3 ~/.local/share/lacodda/kasl/kasl.db

# Common queries
SELECT * FROM workdays ORDER BY date DESC LIMIT 5;
SELECT * FROM tasks WHERE date(timestamp) = date('now');
SELECT COUNT(*) FROM pauses WHERE date(start) = date('now');
```

## Advanced Usage

### Direct SQL Access

Access database directly:
```bash
sqlite3 ~/.local/share/lacodda/kasl/kasl.db
```

Common queries:
```sql
-- Today's work session
SELECT * FROM workdays WHERE date = date('now');

-- Today's tasks
SELECT * FROM tasks WHERE date(timestamp) = date('now');

-- Today's pauses
SELECT * FROM pauses WHERE date(start) = date('now');

-- Task completion statistics
SELECT 
    COUNT(*) as total_tasks,
    SUM(CASE WHEN completeness = 100 THEN 1 ELSE 0 END) as completed,
    AVG(completeness) as avg_completion
FROM tasks 
WHERE date(timestamp) = date('now');
```

### Data Export

Export specific data:
```bash
# Export workdays
sqlite3 kasl.db "SELECT * FROM workdays;" > workdays.csv

# Export tasks with tags
sqlite3 kasl.db "
SELECT t.name, t.completeness, GROUP_CONCAT(tag.name) as tags
FROM tasks t
LEFT JOIN task_tags tt ON t.id = tt.task_id
LEFT JOIN tags tag ON tt.tag_id = tag.id
GROUP BY t.id
ORDER BY t.timestamp DESC;
" > tasks_with_tags.csv
```

### Custom Queries

Create custom reports:
```sql
-- Weekly summary
SELECT 
    date,
    COUNT(*) as tasks,
    AVG(completeness) as avg_completion
FROM tasks 
WHERE date(timestamp) >= date('now', '-7 days')
GROUP BY date
ORDER BY date;

-- Tag usage statistics
SELECT 
    tag.name,
    COUNT(*) as usage_count
FROM tags tag
JOIN task_tags tt ON tag.id = tt.tag_id
GROUP BY tag.id
ORDER BY usage_count DESC;
```

## Related pages

- [`pauses`](/reference/pauses/) - protected pauses and the `--keep`/`--reason` flags behind the `pauses` columns
- [`task`](/reference/task/) - task commands, including `remove`
- [`inbox`](/reference/inbox/) - the Jira inbox commands backed by `jira_inbox` and `jira_statuses`
- [`export`](/reference/export/) - export formats and data types, including `export all`
- [Configuration](/concepts/configuration/) - config keys referenced by pauses, reports, and Jira sync
