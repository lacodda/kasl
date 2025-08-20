# Getting Started

This guide will help you get up and running with kasl quickly.

## Installation

### Quick Install (Recommended)

Install kasl using curl:
```bash
sh -c "$(curl -fsSL https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.sh)"
```

Or using wget:
```bash
sh -c "$(wget https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.sh -O -)"
```

### Build from Source

Requirements:
- Rust 1.70 or higher
- Git

```bash
git clone https://github.com/lacodda/kasl.git
cd kasl
cargo build --release
cargo install --path .
```

### Verify Installation

Check that kasl is installed correctly:
```bash
kasl --version
```

## Initial Configuration

### Interactive Setup

Run the interactive configuration wizard:
```bash
kasl init
```

This will guide you through setting up:
- Monitor settings (pause thresholds, activity detection)
- API integrations (GitLab, Jira, SiServer)
- Server configuration for report submission

### Manual Configuration

Configuration files are stored in:
- **Windows**: `%LOCALAPPDATA%\lacodda\kasl\config.json`
- **macOS**: `~/Library/Application Support/lacodda/kasl/config.json`
- **Linux**: `~/.local/share/lacodda/kasl/config.json`

Example configuration:
```json
{
  "monitor": {
    "min_pause_duration": 20,
    "pause_threshold": 60,
    "poll_interval": 500,
    "activity_threshold": 30,
    "min_work_interval": 10
  },
  "si": {
    "login": "your.email@company.com",
    "auth_url": "https://auth.company.com",
    "api_url": "https://api.company.com"
  },
  "gitlab": {
    "access_token": "your-token",
    "api_url": "https://gitlab.com"
  },
  "jira": {
    "login": "your.email@company.com",
    "api_url": "https://jira.company.com"
  }
}
```

## First Steps

### 1. Start Activity Monitoring

Begin tracking your work sessions:
```bash
# Start monitoring in the background
kasl watch

# Or run in foreground for debugging
kasl watch --foreground
```

### 2. Enable Autostart (Optional)

Configure kasl to start automatically on system boot:
```bash
kasl autostart enable
```

### 3. Create Your First Task

Add a task to track your work:
```bash
kasl task --name "Set up kasl" --completeness 100
```

### 4. View Your Report

Check your work summary:
```bash
kasl report
```

## Daily Workflow

### Morning Routine

1. **Check yesterday's report** (if needed):
   ```bash
   kasl report --last
   ```

2. **Create today's tasks**:
   ```bash
   kasl task --name "Code review" --completeness 0
   kasl task --name "Team meeting" --completeness 0
   ```

3. **Start monitoring** (if not already running):
   ```bash
   kasl watch
   ```

### During the Day

1. **Update task progress**:
   ```bash
   kasl task --edit 1  # Edit task by ID
   ```

2. **Add new tasks as needed**:
   ```bash
   kasl task --name "Bug fix" --completeness 0
   ```

3. **View current status**:
   ```bash
   kasl task --show  # Show today's tasks
   ```

### End of Day

1. **View today's report**:
   ```bash
   kasl report
   ```

2. **Submit report** (if configured):
   ```bash
   kasl report --send
   ```

3. **End workday manually** (if needed):
   ```bash
   kasl end
   ```

## Common Tasks

### Task Management

```bash
# Create a task
kasl task --name "Task name" --comment "Description" --completeness 0

# List tasks
kasl task --show

# Edit a task
kasl task --edit 1

# Delete a task
kasl task --delete 1

# Use templates
kasl task --from-template
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

### Monthly Summary

```bash
# View monthly summary
kasl sum

# Submit monthly report
kasl sum --send
```

## Troubleshooting

### Check if Monitoring is Running

```bash
kasl watch --stop  # Stop any running instances
kasl watch --foreground  # Start in foreground to see logs
```

### Debug Mode

Enable debug logging:
```bash
KASL_DEBUG=1 kasl watch --foreground
```

### Reset Configuration

If you need to start over:
```bash
kasl init --delete
```

## Next Steps

- Read the [Features](./features.md) guide to learn about advanced capabilities
- Explore [API Integrations](./api-integrations.md) for external service connections
- Check the [Configuration](./configuration.md) guide for detailed settings
- Review [Commands](./commands/index.md) for complete command reference
