---
title: "Commands"
sidebar:
  order: 6
---

kasl provides a comprehensive set of commands for work activity tracking, task management, and reporting.

## Command Overview

### Core Commands

- **[`setup`](/reference/setup/)** - Set up application configuration
- **[`watch`](/reference/watch/)** - Start activity monitoring
- **[`task`](/reference/task/)** - Manage tasks and work items
- **[`report`](/reference/report/)** - Generate and submit reports
- **[`sum`](/reference/sum/)** - View monthly summaries
- **[`end`](/reference/end/)** - Close today's workday by hand

### Data Management

- **[`export`](/reference/export/)** - Export data to various formats
- **[`pauses`](/reference/pauses/)** - View pauses and record ones the monitor missed

### Incoming work

- **[`inbox`](/reference/inbox/)** - Jira issues assigned to you, polled in the background

### Organization

- **[`tag`](/reference/tag/)** - Manage task tags and categorization
- **[`template`](/reference/template/)** - Create and use task templates

### System Integration

- **[`server`](/reference/server/)** - Connect to a team kasl-server, send your days to it, and read what it stores about you
- **[`autostart`](/reference/autostart/)** - Configure automatic startup
- **[`self-update`](/reference/self-update/)** - Update application to latest version
- **[`completions`](/reference/completions/)** - Print a shell completion script

## Quick Reference

### Daily Workflow

```bash
# Start monitoring (if not already running)
kasl watch

# Create today's tasks
kasl task add --name "Code review" --completeness 0
kasl task add --name "Team meeting" --completeness 0

# Update task progress
kasl task edit 1

# View today's report
kasl report

# Submit report (if configured)
kasl report --send
```

### Task Management

```bash
# Create tasks
kasl task add --name "Task name" --comment "Description" --completeness 0

# List tasks
kasl task list
kasl task list --all

# Edit tasks
kasl task edit 1
kasl task edit

# Remove tasks
kasl task remove 1 2 3
kasl task remove --today
```

### Recording Missed Absences

```bash
# Record an hour-long lunch the monitor missed
kasl pauses add --start 13:00 --minutes 60 --reason "lunch"

# Record a short absence that must survive the duration filter
kasl pauses add --start 16:20 --minutes 10 --keep

# Review the day
kasl pauses list
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
kasl tag add "urgent" --color "red"
kasl tag add "backend" --color "blue"

# List tags
kasl tag list

# Assign tags to tasks
kasl task add --name "Fix bug" --tags "urgent,backend"

# Filter by tags
kasl task list --tag "urgent"
```

### Templates

```bash
# Create template
kasl template add --name "daily-standup"

# Use template
kasl task add --from-template
kasl task add --template "daily-standup"
```

## Command Categories

### Activity Monitoring
Commands for tracking work sessions and activity:
- `watch` - Core monitoring functionality
- `pauses` - View pauses and record ones the monitor missed

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
- `setup` - Initial setup and configuration
- `server` - Connection to a team server, the queue of days owed to it, and its privacy manifest
- `autostart` - System integration
- `self-update` - Application updates

## Getting Help

### Omitting an identifier

Commands that act on one thing - an inbox issue, a pause, a tag, a task - take
its identifier as an argument. Leave the argument out on a terminal and kasl
lists what is there and lets you choose, showing summaries and durations rather
than bare keys and ids.

This is a convenience for interactive use only. With no terminal attached -
under the watch daemon, in CI, behind a pipe - the same command fails and names
the argument it wanted, so an unattended run reports the problem instead of
hanging on a prompt nobody can answer.

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
kasl task add --help
kasl report --help
```

### Interactive Mode

Some commands support interactive mode:
```bash
# Interactive task creation
kasl task

# Interactive template selection
kasl task add --from-template

# Interactive task editing
kasl task edit
```

## Options you will meet everywhere

- `--help` on any command or subcommand prints its own help; `kasl --version`
  prints the version.
- `--date <YYYY-MM-DD|today>` selects the day for `report`, `export` and
  `pauses`.
- `-y`/`--yes` skips the confirmation on destructive commands, which is what
  makes them usable from a script.
- Commands that would otherwise prompt refuse with an explanatory error when
  there is no terminal, rather than waiting for an answer nobody can give.

Debug logging is a environment variable, not a flag:

```bash
KASL_DEBUG=1 kasl watch --foreground
RUST_LOG=kasl=debug kasl task find
```

## Related pages

- [Getting Started](/getting-started/) - installing kasl and the first run
- [Configuration](/concepts/configuration/) - every setting in one place
- [API Integrations](/concepts/api-integrations/) - GitLab, Jira and report submission
