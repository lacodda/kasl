# Features

kasl provides comprehensive work activity tracking and task management capabilities. This guide covers all major features and their usage.

## 🔍 Activity Monitoring

### Automatic Work Session Detection

kasl automatically detects when you start and end your workday based on your activity patterns:

- **Activity Threshold**: Configurable duration of continuous activity required to start a workday
- **Smart Detection**: Distinguishes between brief interactions and actual work sessions
- **Background Operation**: Runs silently without interrupting your workflow

### Break Detection

Intelligent pause detection that adapts to your work patterns:

- **Inactivity Threshold**: Configurable time before a pause is detected
- **Minimum Pause Duration**: Only records breaks longer than the specified duration
- **Automatic Resume**: Detects when you return to work and resumes tracking

### Configuration Options

```json
{
  "monitor": {
    "min_pause_duration": 20,    // Minutes - minimum break to record
    "pause_threshold": 60,       // Seconds - inactivity before pause
    "poll_interval": 500,        // Milliseconds - activity check interval
    "activity_threshold": 30,    // Seconds - activity before workday start
    "min_work_interval": 10      // Minutes - minimum work interval
  }
}
```

## 🎯 Productivity Optimization

### Recording Missed Absences

The monitor only sees keyboard and mouse, so an absence spent away from the machine leaves no trace. Manual pauses put it on the record:

- **Explicit Times**: You state when the absence began and how long it lasted
- **Protection**: `--keep` exempts an entry from threshold filtering and merging
- **Conflict Prevention**: An entry overlapping a recorded pause is rejected

```bash
# Record an hour-long lunch the monitor missed
kasl pauses add --start 13:00 --minutes 60 --reason "lunch"

# Record a short absence that must survive the duration filter
kasl pauses add --start 16:20 --minutes 10 --keep
```

### Productivity Metrics

Real-time productivity tracking and validation:

- **Threshold Validation**: Configurable minimum productivity for report submission
- **Low-Productivity Warning**: Flags a day that falls below the threshold
- **Report Integration**: Blocks low-productivity report submission

```text
Available Work Time = Workday Length - Long Pauses
Net Work Time       = Available Work Time - Short Pauses
Productivity        = Net Work Time / Available Work Time * 100
```

Long pauses - detected absences at or above `min_pause_duration`, plus every manual pause - leave the denominator entirely. Short pauses lower the numerator only.

### Configuration

```json
{
  "productivity": {
    "min_productivity_threshold": 75.0,
    "workday_hours": 8.0,
    "min_workday_fraction_before_suggest": 0.5
  }
}
```

## 📋 Task Management

### CRUD Operations

Complete task lifecycle management:

```bash
# Create tasks
kasl task --name "Review PR" --comment "Security review" --completeness 0

# Read tasks
kasl task --show
kasl task --show --all  # Show all tasks, not just today's

# Update tasks
kasl task --edit 1  # Interactive editing
kasl task --edit-interactive  # Edit multiple tasks

# Delete tasks
kasl task --delete 1
kasl task --delete-today  # Delete all today's tasks
```

### Task Templates

Save frequently used tasks as reusable templates:

```bash
# Create a template
kasl template create --name "daily-standup"

# Use a template
kasl task --from-template
kasl task --template "daily-standup"
```

### Tagging System

Organize tasks with custom tags and colors:

```bash
# Create tags
kasl tag create --name "urgent" --color "red"
kasl tag create --name "backend" --color "blue"

# Assign tags to tasks
kasl task --name "Fix bug" --tags "urgent,backend"

# Filter by tags
kasl task --show --tag "urgent"
```

### Progress Tracking

Track task completion with percentage-based progress:

- **0%**: Not started
- **1-99%**: In progress
- **100%**: Completed

```bash
kasl task --name "Feature implementation" --completeness 25
kasl task --edit 1  # Update progress interactively
```

## 📊 Reporting & Analytics

### Daily Reports

Comprehensive daily work summaries:

```bash
# View today's report
kasl report

# View yesterday's report
kasl report --last

# Submit report to configured API
kasl report --send
```

Report includes:
- Work intervals with precise timing
- Break periods and durations
- Task completion status
- Productivity metrics
- Total work hours

### Monthly Summaries

Aggregated monthly statistics:

```bash
# View monthly summary
kasl sum

# Submit monthly report
kasl sum --send
```

Features:
- Daily work hour totals
- Average daily hours
- Productivity trends
- Working day count
- Rest day integration

### Productivity Metrics

Calculate and track productivity:

- **Gross Time**: Total time from start to end
- **Net Time**: Actual work time minus breaks
- **Productivity Percentage**: Net time / Gross time
- **Break Analysis**: Break frequency and duration patterns

### Short Interval Filtering

Automatically filter out brief work periods for cleaner reporting:

- **Automatic Detection**: Short intervals are filtered based on `min_work_interval` configuration
- **Display-Level Filtering**: Original data remains intact in the database
- **Consistent Behavior**: Same filtering applies to both display and API submission
- **User Notification**: Information about filtered intervals is shown in reports

```bash
# Reports automatically filter short intervals
kasl report

# Configuration controls the filtering threshold
# (set via min_work_interval in monitor config)
```

## ⚙️ Advanced Features

### Correcting the Record

Record an absence the monitor missed, or remove a pause entered by mistake:

```bash
# Record an absence on a given date
kasl pauses add --date 2026-08-07 --start 11:30 --minutes 45

# Keep a short absence that the duration filter would drop
kasl pauses add --start 16:20 --minutes 10 --keep

# Remove a pause by id
kasl pauses remove 42
```

Features:
- Explicit times - nothing is inferred
- Overlapping entries are rejected
- `--keep` exempts an entry from filtering and merging
- Date-specific entries

### Data Export

Export data in multiple formats:

```bash
# Export to CSV
kasl export --format csv --output report.csv

# Export to JSON
kasl export --format json --output data.json

# Export to Excel
kasl export --format excel --output report.xlsx

# Export all data
kasl export all --format json
```

Supported formats:
- **CSV**: Universal compatibility
- **JSON**: Structured data
- **Excel**: Formatted reports with multiple sheets

### Database Management

Safe database operations:

```bash
# View migration status (debug builds only)
kasl migrations status

# View migration history (debug builds only)
kasl migrations history
```

Features:
- Automatic schema migrations
- Safe database updates
- Migration history tracking
- Rollback capabilities (debug builds)

## 🔗 API Integrations

### GitLab Integration

Import commits as completed tasks:

```bash
# Configure GitLab
kasl init  # Interactive setup

# Find tasks from GitLab
kasl task --find
```

Features:
- Automatic commit import
- User activity tracking
- Repository-specific filtering
- Commit message parsing

### Jira Integration

Import completed issues:

```bash
# Configure Jira
kasl init  # Interactive setup

# Find tasks from Jira
kasl task --find
```

Features:
- Issue status tracking
- Automatic completion detection
- Custom field mapping
- Project-specific filtering

### SiServer Integration

Submit reports to internal systems:

```bash
# Configure SiServer
kasl init  # Interactive setup

# Submit daily report
kasl report --send

# Submit monthly report
kasl sum --send
```

Features:
- Secure authentication
- Report formatting
- Error handling
- Retry logic

## 🚀 System Integration

### Autostart Support

Configure automatic startup:

```bash
# Enable autostart
kasl autostart enable

# Check status
kasl autostart status

# Disable autostart
kasl autostart disable
```

Platform support:
- **Windows**: Task Scheduler and Registry
- **macOS**: LaunchAgent (planned)
- **Linux**: systemd user service (planned)

### Background Monitoring

Silent background operation:

```bash
# Start background monitoring
kasl watch

# Stop monitoring
kasl watch --stop

# Check if running
kasl watch --status
```

Features:
- Daemon process management
- Automatic restart on failure
- Resource optimization
- Signal handling

### Debug Logging

Comprehensive debugging capabilities:

```bash
# Enable debug mode
KASL_DEBUG=1 kasl watch --foreground

# Use Rust logging
RUST_LOG=kasl=debug kasl report

# Trace level logging
RUST_LOG=kasl=trace kasl watch
```

Log levels:
- **Error**: Critical issues
- **Warn**: Important warnings
- **Info**: General information
- **Debug**: Detailed debugging
- **Trace**: Maximum verbosity

## 📱 Cross-Platform Support

### Operating Systems

- **Windows 10+**: Full native support
- **macOS 10.15+**: Full native support
- **Linux**: Full native support

### Architecture Support

- **x86_64**: Primary target
- **ARM64**: Planned support

### Installation Methods

- **Binary releases**: Pre-compiled executables
- **Package managers**: Platform-specific packages
- **Source compilation**: From Rust source code

## 🔒 Security Features

### Data Protection

- **Local storage**: All data stored locally
- **Encrypted credentials**: API tokens encrypted at rest
- **Session management**: Secure session handling
- **Permission isolation**: Minimal system permissions

### Privacy

- **No telemetry**: No data sent without explicit consent
- **Local processing**: All analysis done locally
- **Configurable sharing**: Control over data export
- **Audit trails**: Complete operation logging

## 📈 Performance

### Resource Usage

- **Memory**: ~50MB RAM
- **CPU**: Minimal background usage
- **Storage**: ~10MB application + data
- **Network**: Optional API calls only

### Optimization

- **Efficient polling**: Configurable activity check intervals
- **Database optimization**: Indexed queries and transactions
- **Memory management**: Automatic cleanup and garbage collection
- **Background processing**: Non-blocking operations


