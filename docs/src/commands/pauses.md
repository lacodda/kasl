# `pauses` Command

The `pauses` command provides detailed views of automatically detected and manually recorded breaks during work sessions. It helps users understand their break patterns and verify the accuracy of automatic pause detection.

## Usage

```bash
kasl pauses [OPTIONS]
```

## Options

- `-d, --date <DATE>`: Date to fetch pauses for (default: `today`)
  - `today`: Current date
  - `YYYY-MM-DD`: Specific date in ISO format

- `-m, --min-duration <MINUTES>`: Minimum pause duration filter in minutes
  - When specified, only pauses longer than this duration will be displayed
  - Overrides the default minimum pause duration from configuration
  - Useful for filtering out brief interruptions and focusing on significant breaks
  - If not specified, uses the configured `min_pause_duration` setting

## Display Format

The output includes:

- **Pause List**: Each pause with start time, end time, and duration
- **Total Time**: Sum of all pause durations for the day
- **Pause Count**: Number of breaks recorded

## Data Sources

Pause data comes from:

- **Automatic Detection**: Monitor-recorded inactivity periods
- **Manual Adjustments**: User-added pauses via `adjust` command
- **Time Corrections**: Modified pause times from manual adjustments

## Examples

### Basic Usage

```bash
# Show today's pauses
kasl pauses

# Show pauses for specific date
kasl pauses --date 2024-12-15

# Show pauses for yesterday
kasl pauses --date 2024-12-14
```

### Duration Filtering

```bash
# Show only pauses longer than 30 minutes
kasl pauses --min-duration 30

# Show only significant breaks (longer than 15 minutes)
kasl pauses --min-duration 15

# Filter specific date with duration
kasl pauses --date 2024-12-15 --min-duration 10
```

### Analysis Examples

```bash
# Review break patterns for the week
kasl pauses --date 2024-12-15
kasl pauses --date 2024-12-16
kasl pauses --date 2024-12-17

# Focus on significant breaks only
kasl pauses --min-duration 30

# Compare different duration thresholds
kasl pauses --min-duration 5
kasl pauses --min-duration 15
kasl pauses --min-duration 30
```

## Sample Output

### Today's Pauses
```
December 15, 2024

+--------------+-------+-------+----------+
| ID           | START | END   | DURATION |
+--------------+-------+-------+----------+
| 1            | 10:30 | 10:45 | 00:15    |
| 2            | 12:00 | 13:00 | 01:00    |
| 3            | 15:15 | 15:30 | 00:15    |
| 4            | 16:45 | 17:00 | 00:15    |
|              |       |       |          |
| TOTAL        |       |       | 01:45    |
+--------------+-------+-------+----------+
```

### Filtered Output (min-duration: 30)
```
December 15, 2024

+--------------+-------+-------+----------+
| ID           | START | END   | DURATION |
+--------------+-------+-------+----------+
| 1            | 12:00 | 13:00 | 01:00    |
|              |       |       |          |
| TOTAL        |       |       | 01:00    |
+--------------+-------+-------+----------+
```

### No Pauses Found
```
December 15, 2024

+--------------+-------+-------+----------+
| ID           | START | END   | DURATION |
+--------------+-------+-------+----------+
+--------------+-------+-------+----------+
```

## Use Cases

### Daily Break Review

```bash
# Review today's break patterns
kasl pauses

# Check if lunch break was recorded
kasl pauses --min-duration 30
```

### Break Pattern Analysis

```bash
# Analyze break patterns over multiple days
for date in 2024-12-15 2024-12-16 2024-12-17; do
    echo "=== $date ==="
    kasl pauses --date $date
    echo
done
```

### Monitoring Verification

```bash
# Verify that automatic pause detection is working
kasl pauses --min-duration 5

# Check for missed breaks
kasl pauses --date 2024-12-15
```

### Manual Pause Review

```bash
# Review manually added pauses
kasl pauses --date 2024-12-15

# Compare with automatic detection
kasl report --date 2024-12-15
```

## Duration Filtering

### Understanding Duration Filters

The `--min-duration` option helps focus on significant breaks:

- **5 minutes**: Very short breaks (coffee, bathroom)
- **15 minutes**: Standard short breaks
- **30 minutes**: Significant breaks (lunch, meetings)
- **60 minutes**: Major breaks (lunch, appointments)

### Configuration Integration

The command respects your configuration settings:

```bash
# Check your current minimum pause duration setting
kasl init --show-config

# The pauses command will use this setting by default
kasl pauses
```

## Integration with Other Commands

The `pauses` command works with other kasl commands:

- **`report`**: Compare pause data with overall workday summary
- **`adjust`**: Add manual pauses that appear in pause listings
- **`watch`**: Automatic pause detection that feeds into pause data
- **`export`**: Export pause data for external analysis

## Troubleshooting

### Common Issues

**No pauses found**
```bash
# Check if workday exists
kasl report --date 2024-12-15

# Try without duration filter
kasl pauses --date 2024-12-15

# Check monitoring configuration
kasl init --show-config
```

**Unexpected pause durations**
```bash
# Review pause detection settings
kasl init --show-config

# Check for manual adjustments
kasl adjust --date 2024-12-15
```

**Missing breaks**
```bash
# Add manual pause if automatic detection missed it
kasl adjust --mode pause --minutes 30 --date 2024-12-15

# Verify the pause was added
kasl pauses --date 2024-12-15
```

### Data Validation

```bash
# Cross-reference with workday report
kasl report --date 2024-12-15
kasl pauses --date 2024-12-15

# Export data for external verification
kasl export --date 2024-12-15 --format json
```

## Best Practices

### Regular Review

1. **Review daily pauses** to understand break patterns
2. **Verify automatic detection** is working correctly
3. **Add manual pauses** for missed breaks
4. **Analyze patterns** to improve work habits

### Break Management

1. **Use appropriate duration filters** for different analysis needs
2. **Document manual pauses** with clear descriptions
3. **Review break patterns** to optimize productivity
4. **Ensure compliance** with break requirements

### Data Quality

1. **Verify pause accuracy** regularly
2. **Add missing breaks** promptly
3. **Review monitoring settings** if detection is poor
4. **Keep pause data consistent** with workday records

## Related Commands

- **[`report`](./report.md)** - View complete workday summary including pauses
- **[`adjust`](./adjust.md)** - Add manual pauses and adjust recorded times
- **[`watch`](./watch.md)** - Monitor activity and detect automatic pauses
- **[`export`](./export.md)** - Export pause data for external analysis
