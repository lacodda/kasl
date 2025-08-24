# Database

kasl uses SQLite as its local database for storing work sessions, tasks, and configuration data.

## Overview

The database provides:
- **Local Storage**: All data stored locally for privacy
- **ACID Compliance**: Reliable data integrity
- **Migration System**: Safe schema updates
- **Cross-Platform**: Works on all supported platforms

## Database Location

Database files are stored in platform-specific locations:

- **Windows**: `%LOCALAPPDATA%\lacodda\kasl\kasl.db`
- **macOS**: `~/Library/Application Support/lacodda/kasl/kasl.db`
- **Linux**: `~/.local/share/lacodda/kasl/kasl.db`

## Schema Overview

### Tables

#### `workdays`
Stores daily work session information:
```sql
CREATE TABLE workdays (
    id INTEGER PRIMARY KEY,
    date TEXT UNIQUE NOT NULL,
    start TEXT NOT NULL,
    end TEXT
);
```

#### `pauses`
Stores break periods during work sessions:
```sql
CREATE TABLE pauses (
    id INTEGER PRIMARY KEY,
    start TEXT NOT NULL,
    end TEXT,
    duration INTEGER
);
```

#### `tasks`
Stores task information and metadata:
```sql
CREATE TABLE tasks (
    id INTEGER PRIMARY KEY,
    task_id INTEGER DEFAULT 0,
    timestamp TEXT DEFAULT CURRENT_TIMESTAMP,
    name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER DEFAULT 100,
    excluded_from_search BOOLEAN DEFAULT FALSE
);
```

#### `tags`
Stores task categorization tags:
```sql
CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    color TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);
```

#### `task_tags`
Links tasks to tags (many-to-many relationship):
```sql
CREATE TABLE task_tags (
    task_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, tag_id)
);
```

#### `task_templates`
Stores reusable task templates:
```sql
CREATE TABLE task_templates (
    id INTEGER PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    task_name TEXT NOT NULL,
    comment TEXT,
    completeness INTEGER DEFAULT 0,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);
```

#### `migrations`
Tracks database schema version:
```sql
CREATE TABLE migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT DEFAULT CURRENT_TIMESTAMP
);
```

## Data Types

### Timestamps
- **Format**: ISO 8601 (`YYYY-MM-DD HH:MM:SS`)
- **Timezone**: Local system time
- **Storage**: TEXT for human readability

### Dates
- **Format**: ISO 8601 (`YYYY-MM-DD`)
- **Storage**: TEXT for consistency

### Durations
- **Unit**: Seconds
- **Storage**: INTEGER for efficient calculations

### Booleans
- **Storage**: INTEGER (0 = false, 1 = true)
- **SQLite standard**: No native boolean type

## Migration System

### Automatic Migrations

Migrations run automatically on startup:
```bash
kasl watch  # Migrations run automatically
```

### Manual Migration Management

Debug builds provide migration commands:
```bash
# Check migration status
kasl migrations status

# View migration history
kasl migrations history
```

### Migration Process

1. **Version Check**: Compare current vs. target version
2. **Migration Selection**: Find pending migrations
3. **Transaction**: Apply migrations in transaction
4. **Version Update**: Update migration table
5. **Rollback**: Rollback on failure

### Migration Safety

- **Transactions**: All migrations run in transactions
- **Idempotency**: Safe to run multiple times
- **Rollback**: Automatic rollback on failure
- **Versioning**: Strict version ordering

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

# Import data
# (Manual import not yet implemented)
```

### Cleanup

Remove old data:
```bash
# Delete specific tasks
kasl task --delete 1 2 3

# Delete all today's tasks
kasl task --delete-today

# Delete old pauses (manual SQL)
sqlite3 kasl.db "DELETE FROM pauses WHERE start < date('now', '-30 days');"
```

## Performance

### Indexes

Automatic indexes for performance:
```sql
-- Workdays table
CREATE INDEX idx_workdays_date ON workdays(date);

-- Tasks table
CREATE INDEX idx_tasks_timestamp ON tasks(timestamp);
CREATE INDEX idx_tasks_completeness ON tasks(completeness);

-- Pauses table
CREATE INDEX idx_pauses_start ON pauses(start);
```

### Optimization

- **Connection Pooling**: Efficient connection management
- **Prepared Statements**: Reused query plans
- **Transactions**: Batch operations for performance
- **Memory Management**: Automatic cleanup

### Monitoring

Check database performance:
```bash
# Enable SQLite logging
RUST_LOG=kasl=debug kasl report

# Check database size
ls -lh ~/.local/share/lacodda/kasl/kasl.db

# Analyze database
sqlite3 kasl.db "ANALYZE;"
```

## Security

### File Permissions

Secure database file:
```bash
# Linux/macOS
chmod 600 ~/.local/share/lacodda/kasl/kasl.db
chmod 700 ~/.local/share/lacodda/kasl/
```

### Data Privacy

- **Local Storage**: No data sent to external servers
- **Encryption**: Consider filesystem encryption
- **Access Control**: Restrict file permissions
- **Audit Trail**: Complete operation logging

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

**Problem**: Migration failures
```bash
# Check migration status
kasl migrations status

# View error logs
RUST_LOG=kasl=debug kasl watch --foreground
```

### Debug Database

Enable SQLite debugging:
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

