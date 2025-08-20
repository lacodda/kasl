# Commands

kasl provides a comprehensive set of commands for work activity tracking, task management, and reporting.

## Command Overview

### Core Commands

- **[`init`](./init.md)** - Initialize application configuration
- **[`watch`](./watch.md)** - Start activity monitoring
- **[`task`](./task.md)** - Manage tasks and work items
- **[`report`](./report.md)** - Generate and submit reports
- **[`sum`](./sum.md)** - View monthly summaries

### Data Management

- **[`export`](./export.md)** - Export data to various formats
- **[`adjust`](./adjust.md)** - Adjust work times and add pauses
- **[`pauses`](./pauses.md)** - View recorded breaks and pauses

### Organization

- **[`tag`](./tag.md)** - Manage task tags and categorization
- **[`template`](./template.md)** - Create and use task templates

### System Integration

- **[`autostart`](./autostart.md)** - Configure automatic startup
- **[`update`](./update.md)** - Update application to latest version

## Quick Reference

### Daily Workflow

```bash
# Start monitoring (if not already running)
kasl watch

# Create today's tasks
kasl task --name "Code review" --completeness 0
kasl task --name "Team meeting" --completeness 0

# Update task progress
kasl task --edit 1

# View today's report
kasl report

# Submit report (if configured)
kasl report --send
```

### Task Management

```bash
# Create tasks
kasl task --name "Task name" --comment "Description" --completeness 0

# List tasks
kasl task --show
kasl task --show --all

# Edit tasks
kasl task --edit 1
kasl task --edit-interactive

# Delete tasks
kasl task --delete 1 2 3
kasl task --delete-today
```

### Time Adjustments

```bash
# Adjust work start time
kasl adjust --mode start --minutes 30

# Add a pause
kasl adjust --mode pause --minutes 15

# Adjust work end time
kasl adjust --mode end --minutes 20
```

### Data Export

```bash
# Export today's data
kasl export --format csv

# Export all data
kasl export all --format json

# Export to specific file
kasl export --output my_report.csv
```

### Tag Management

```bash
# Create tags
kasl tag create --name "urgent" --color "red"
kasl tag create --name "backend" --color "blue"

# List tags
kasl tag list

# Assign tags to tasks
kasl task --name "Fix bug" --tags "urgent,backend"

# Filter by tags
kasl task --show --tag "urgent"
```

### Templates

```bash
# Create template
kasl template create --name "daily-standup"

# Use template
kasl task --from-template
kasl task --template "daily-standup"
```

## Command Categories

### Activity Monitoring
Commands for tracking work sessions and activity:
- `watch` - Core monitoring functionality
- `adjust` - Time corrections and adjustments
- `pauses` - Break period management

### Task Management
Commands for organizing and tracking work items:
- `task` - Complete task lifecycle management
- `tag` - Task categorization and organization
- `template` - Reusable task templates

### Reporting & Analytics
Commands for generating insights and reports:
- `report` - Daily work reports
- `sum` - Monthly summaries and statistics
- `export` - Data export for external analysis

### System Management
Commands for application configuration and maintenance:
- `init` - Initial setup and configuration
- `autostart` - System integration
- `update` - Application updates

## Getting Help

### Command Help

Get help for any command:
```bash
kasl --help
kasl <command> --help
```

### Examples

View command examples:
```bash
# Show all available commands
kasl --help

# Show specific command help
kasl task --help
kasl report --help
```

### Interactive Mode

Some commands support interactive mode:
```bash
# Interactive task creation
kasl task

# Interactive template selection
kasl task --from-template

# Interactive task editing
kasl task --edit-interactive
```

## Command Options

### Global Options

Most commands support these global options:
- `--help` - Show command help
- `--version` - Show version information

### Common Options

Many commands support these common options:
- `--date` - Specify date (YYYY-MM-DD or 'today')
- `--output` - Specify output file
- `--format` - Specify output format

### Debug Options

Debug options for troubleshooting:
- `--foreground` - Run in foreground mode
- `--debug` - Enable debug logging

## Best Practices

### Command Organization

1. **Use templates** for frequently created tasks
2. **Use tags** for task categorization
3. **Regular exports** for data backup
4. **Monitor configuration** for optimal detection

### Workflow Integration

1. **Start monitoring** at the beginning of your workday
2. **Create tasks** as you plan your work
3. **Update progress** throughout the day
4. **Review reports** at the end of the day

### Data Management

1. **Regular backups** using export functionality
2. **Clean up old data** periodically
3. **Validate data** using report commands
4. **Monitor database** size and performance