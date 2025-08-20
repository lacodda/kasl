# kasl: Key Activity Synchronization and Logging 🕒

<p align="center">
  <img src="https://raw.githubusercontent.com/lacodda/kasl/main/kasl.webp" width="320" alt="kasl">
</p>

## Overview 📖

kasl is a comprehensive command-line utility designed to streamline work activity tracking, task management, and productivity reporting. It automatically monitors your work sessions, tracks breaks, manages tasks, and generates detailed reports for better productivity insights.

## Key Features 🌟

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
- **Database migrations** - Safe schema updates when upgrading
- **API integrations** - Connect with GitLab, Jira, and custom APIs
- **Autostart support** - Start monitoring automatically on system boot
- **Debug logging** - Detailed logs for troubleshooting

## Quick Start 🚀

### Installation

Install kasl using curl:
```bash
sh -c "$(curl -fsSL https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.sh)"
```

Or using wget:
```bash
sh -c "$(wget https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.sh -O -)"
```

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

# Submit daily report
kasl report --send
```

## System Requirements 💻

- **Operating System**: Windows 10+, macOS 10.15+, or Linux
- **Architecture**: x86_64
- **Memory**: 50MB RAM
- **Storage**: 10MB disk space
- **Network**: Optional (for API integrations and updates)

## License 📄

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Support 📞

- 📧 Email: lahtachev@gmail.com
- 🐛 Issues: [GitHub Issues](https://github.com/lacodda/kasl/issues)
- 📖 Documentation: [kasl.lacodda.com](https://kasl.lacodda.com)

---

Made with ❤️ by [Kirill Lakhtachev](https://lacodda.com)