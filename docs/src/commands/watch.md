# `watch` Command

The `watch` command is the core monitoring functionality of kasl. It tracks user activity to automatically detect work sessions, breaks, and workday boundaries. The command can run in background daemon mode for daily use or in foreground mode for debugging and development.

## Usage

```bash
kasl watch [OPTIONS]
```

## Options

- `--foreground`: Run the watcher in the foreground for debugging
  - Provides real-time feedback about detected activity
  - Shows pause events and workday state changes
  - Useful for testing configuration changes and troubleshooting

- `--stop`, `-s`: Stop any running background watcher process
  - Terminates the background daemon if it's currently running
  - Safely closes database connections and cleans up system resources
  - Useful before system shutdown or when restarting with new configuration

## Operating Modes

### Background Daemon Mode (Default)

```bash
kasl watch
```

Runs silently in the background, perfect for daily use:
- Automatically starts monitoring when you begin working
- Detects work vs. break periods based on configurable thresholds
- Records workday start/end times and pause periods
- Maintains activity state across application restarts

### Foreground Debug Mode

```bash
kasl watch --foreground
```

Runs in the terminal with detailed logging:
- Real-time activity detection feedback
- Pause event notifications
- Workday state change logging
- Configuration testing and validation

### Stop Mode

```bash
kasl watch --stop
```

Terminates background monitoring:
- Stops any running daemon process
- Properly closes database connections
- Cleans up system resources

## How It Works

### Activity Detection

The monitor tracks these input events:
- **Keyboard presses and releases**
- **Mouse button clicks**
- **Mouse movement**
- **Mouse wheel scrolling**

### Work Session Detection

1. **Input Detection**: Monitors keyboard and mouse activity using system APIs
2. **Activity Analysis**: Determines work vs. break periods based on configurable thresholds
3. **Database Recording**: Automatically records workday start/end times and pause periods
4. **State Management**: Maintains activity state across application restarts

### Configuration Settings

The monitor behavior is controlled by configuration settings:

- `pause_threshold`: Seconds of inactivity before recording a pause
- `poll_interval`: Milliseconds between activity checks
- `activity_threshold`: Seconds of activity needed to start a workday
- `min_pause_duration`: Minimum pause length to record (filters noise)

## Examples

### Daily Usage

```bash
# Start background monitoring (recommended for daily use)
kasl watch

# Check if monitoring is running
ps aux | grep kasl
```

### Debugging and Development

```bash
# Run in foreground to see activity detection in real-time
kasl watch --foreground

# Test configuration changes
kasl watch --foreground
# Press Ctrl+C to stop
```

### System Management

```bash
# Stop monitoring before system shutdown
kasl watch --stop

# Restart with new configuration
kasl watch --stop
kasl watch
```

## Database Operations

During monitoring, the system automatically:

- **Creates workday records** when sustained activity is detected
- **Records pause start times** when inactivity threshold is exceeded
- **Records pause end times** when activity resumes
- **Updates workday end times** when monitoring stops

## Signal Handling

The daemon process responds to these signals:
- **SIGTERM**: Graceful shutdown (Unix)
- **SIGINT**: Interrupt signal (Unix)
- **Ctrl+C**: Console interrupt (Windows)

## Troubleshooting

### Common Issues

**Daemon already running**
```bash
# Check if daemon is running
ps aux | grep kasl

# Stop existing daemon
kasl watch --stop

# Start new daemon
kasl watch
```

**Permission issues**
```bash
# Run with elevated permissions if needed
sudo kasl watch --foreground
```

**Configuration errors**
```bash
# Test configuration in foreground mode
kasl watch --foreground
```

### Debug Mode

Enable debug logging for troubleshooting:
```bash
RUST_LOG=kasl=debug kasl watch --foreground
```

### Process Management

```bash
# Check daemon status
ps aux | grep kasl

# View daemon logs (if configured)
tail -f ~/.local/share/lacodda/kasl/kasl.log

# Force stop daemon (if needed)
pkill -f kasl
```

## Integration with Other Commands

The `watch` command works with other kasl commands:

- **`task`**: Create and manage tasks while monitoring
- **`report`**: Generate reports based on monitored data
- **`adjust`**: Manually adjust times recorded by monitoring
- **`pauses`**: View recorded breaks and pauses

## Best Practices

### Daily Workflow

1. **Start monitoring** at the beginning of your workday
2. **Let it run** in the background during work
3. **Stop monitoring** before system shutdown
4. **Review data** using `report` and `pauses` commands

### Configuration

1. **Adjust thresholds** based on your work patterns
2. **Test settings** using foreground mode
3. **Monitor performance** and adjust as needed
4. **Backup configuration** regularly

### Troubleshooting

1. **Use foreground mode** for debugging
2. **Check system permissions** for input device access
3. **Verify database connectivity**
4. **Review logs** for error messages

## Related Commands

- **[`task`](./task.md)** - Manage tasks and work items
- **[`report`](./report.md)** - Generate work reports
- **[`adjust`](./adjust.md)** - Adjust recorded times
- **[`pauses`](./pauses.md)** - View recorded breaks
- **[`init`](./init.md)** - Configure monitoring settings
