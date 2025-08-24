# kasl - Key Activity Synchronization and Logging

<p align="center">
  <img src="https://raw.githubusercontent.com/lacodda/kasl/main/kasl.webp" width="320" alt="kasl">
</p>

<p align="center">
  <a href="https://github.com/lacodda/kasl/releases"><img src="https://img.shields.io/github/v/release/lacodda/kasl?style=flat-square" alt="Release"></a>
  <a href="https://github.com/lacodda/kasl/blob/main/LICENSE"><img src="https://img.shields.io/github/license/lacodda/kasl?style=flat-square" alt="License"></a>
  <a href="https://github.com/lacodda/kasl/actions"><img src="https://img.shields.io/github/actions/workflow/status/lacodda/kasl/release.yml?style=flat-square" alt="Build Status"></a>
  <a href="https://docs.rs/kasl"><img src="https://img.shields.io/docsrs/kasl?style=flat-square" alt="Documentation"></a>
</p>

## Overview 📖

kasl is a comprehensive command-line utility designed to streamline work activity tracking, task management, and productivity reporting. It automatically monitors your work sessions, tracks breaks, manages tasks, and generates detailed reports for better productivity insights.

**Current Version:** 0.8.2

## ✨ Features

### 🔍 Activity Monitoring
- **Automatic work session tracking** - Detects when you start and end your workday
- **Smart break detection** - Automatically records breaks based on inactivity
- **Background monitoring** - Runs silently in the background
- **Cross-platform support** - Works on Windows, macOS, and Linux

### 📋 Task Management
- **CRUD operations** - Create, read, update, and delete tasks
- **Task templates** - Save frequently used tasks as reusable templates
- **Tagging system** - Organize tasks with custom tags and colors
- **Progress tracking** - Track task completion percentage
- **Batch operations** - Edit or delete multiple tasks at once

### 📊 Reporting & Analytics
- **Daily reports** - Comprehensive view of work intervals and tasks
- **Monthly summaries** - Aggregated statistics and productivity metrics
- **Productivity calculation** - Measure actual work time vs. presence time
- **Short interval detection** - Identify and merge fragmented work periods
- **Export capabilities** - Export data to CSV, JSON, or Excel formats

### ⚙️ Advanced Features
- **Time adjustment** - Correct work times with preview before applying
- **Database migrations** - Safe schema updates when upgrading (debug builds only)
- **API integrations** - Connect with GitLab, Jira, and custom APIs
- **Autostart support** - Start monitoring automatically on system boot
- **Debug logging** - Detailed logs for troubleshooting

## 🚀 Installation

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

## 📚 Quick Start

### Initial Setup

```bash
# Configure kasl interactively
kasl init

# Start activity monitoring
kasl watch

# Enable autostart on system boot
kasl autostart enable
```

### Daily Workflow

```bash
# Create a new task
kasl task --name "Review pull requests" --completeness 0

# Update task progress
kasl task --edit 1

# View today's report
kasl report

# Manually end workday (if needed)
kasl end

# Submit daily report
kasl report --send
```

## 📖 Command Reference

### Core Commands

#### `watch` - Activity Monitoring
```bash
# Start monitoring in background
kasl watch

# Run in foreground (debug mode)
kasl watch --foreground

# Stop monitoring
kasl watch --stop
```

#### `task` - Task Management
```bash
# Create task
kasl task --name "Fix bug #123" --comment "High priority" --tags "bug,urgent"

# Create from template
kasl task --template daily-standup
kasl task --from-template  # Interactive selection

# View tasks
kasl task --show              # Today's tasks
kasl task --show --all        # All tasks
kasl task --show --tag urgent # Tasks with specific tag

# Edit tasks
kasl task --edit 5            # Edit by ID
kasl task --edit-interactive  # Batch edit

# Delete tasks
kasl task --delete 1 2 3      # Delete by IDs
kasl task --delete-today      # Delete all today's tasks
```

#### `report` - Report Generation
```bash
# View report
kasl report                      # Today's report
kasl report --last              # Yesterday's report

# Submit reports
kasl report --send              # Send daily report
kasl report --month             # Send monthly summary
```

#### `end` - Manual Workday End
```bash
# Manually end today's workday
kasl end
```

#### `template` - Task Templates
```bash
# Manage templates
kasl template create --name "standup"
kasl template list
kasl template edit standup
kasl template delete standup
kasl template search daily
```

#### `tag` - Tag Management
```bash
# Manage tags
kasl tag create urgent --color red
kasl tag list
kasl tag edit urgent
kasl tag delete personal
kasl tag tasks urgent  # Show tasks with tag
```

#### `export` - Data Export
```bash
# Export data
kasl export report --format csv
kasl export tasks --format json --date 2025-01-15
kasl export summary --format excel -o monthly_report.xlsx
kasl export all --format json  # Export everything
```

#### `adjust` - Time Adjustment
```bash
# Adjust work time
kasl adjust --mode start --minutes 30  # Remove 30 min from start
kasl adjust --mode end --minutes 60    # Remove 1 hour from end
kasl adjust --mode pause --minutes 45  # Add 45-min pause
kasl adjust  # Interactive mode
```

### Utility Commands

#### `sum` - Monthly Summary
```bash
kasl sum  # View monthly working hours summary
```

#### `pauses` - Break Management
```bash
kasl pauses                     # Today's breaks
kasl pauses --date 2025-01-15  # Specific date
kasl pauses --min-duration 10   # Filter by duration
```

#### `autostart` - System Integration
```bash
kasl autostart enable   # Enable autostart
kasl autostart disable  # Disable autostart
kasl autostart status   # Check status
```

#### `update` - Self-Update
```bash
kasl update  # Check and install updates
```

#### `migrations` - Database Management (Debug Only)
```bash
kasl migrations status  # Check database version
kasl migrations history # View migration history
```
**Note:** This command is only available in debug builds.

## ⚙️ Configuration

Configuration file is stored at:
- Windows: `%LOCALAPPDATA%\lacodda\kasl\config.json`
- macOS: `~/Library/Application Support/lacodda/kasl/config.json`
- Linux: `~/.local/share/lacodda/kasl/config.json`

### Configuration Options

```json
{
  "monitor": {
    "min_pause_duration": 20,    // Minutes - minimum break to record
    "pause_threshold": 60,       // Seconds - inactivity before pause
    "poll_interval": 500,        // Milliseconds - activity check interval
    "activity_threshold": 30,    // Seconds - activity before workday start
    "min_work_interval": 10      // Minutes - minimum work interval
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

## 🔍 Debugging

Enable debug logging for troubleshooting:

```bash
# Enable debug mode with full formatting
KASL_DEBUG=1 kasl watch

# Use standard Rust logging
RUST_LOG=kasl=debug kasl report

# Trace level for maximum verbosity
RUST_LOG=kasl=trace KASL_LOG_FORMAT=full kasl watch
```

## 🗄️ Database

kasl uses SQLite for local data storage. The database is located at:
- Windows: `%LOCALAPPDATA%\lacodda\kasl\kasl.db`
- macOS: `~/Library/Application Support/lacodda/kasl/kasl.db`
- Linux: `~/.local/share/lacodda/kasl/kasl.db`

### Backup

Regular backups are recommended:
```bash
# Export all data
kasl export all --format json -o backup_$(date +%Y%m%d).json
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

### Development Setup

```bash
# Clone repository
git clone https://github.com/lacodda/kasl.git
cd kasl

# Run tests
cargo test

# Run with debug logging
KASL_DEBUG=1 cargo run -- watch --foreground

# Build for release
cargo build --release

# Debug build (enables migrations command)
cargo build
```

### Code Style

We maintain consistent code documentation standards. Please refer to our style guide:
- [Documentation Style Guide](https://kasl.lacodda.com/development/style-guide.html) - Complete guide for writing documentation

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built with [Rust](https://www.rust-lang.org/)
- CLI powered by [clap](https://github.com/clap-rs/clap)
- Database management with [rusqlite](https://github.com/rusqlite/rusqlite)
- Excel export via [rust_xlsxwriter](https://github.com/jmcnamara/rust_xlsxwriter)

## 📞 Support

- 📧 Email: lahtachev@gmail.com
- 🐛 Issues: [GitHub Issues](https://github.com/lacodda/kasl/issues)
- 📖 Documentation: [kasl.lacodda.com](https://kasl.lacodda.com)

---

Made with ❤️ by [Kirill Lakhtachev](https://lacodda.com)