# `sum` Command

The `sum` command generates comprehensive monthly reports showing daily work hours, productivity metrics, and calendar integration with company rest days. It provides both detailed daily breakdowns and aggregate statistics for the current month.

## Usage

```bash
kasl sum [OPTIONS]
```

## Options

- `--send`: Submit the monthly summary report
  - Generates and submits the summary to the configured reporting API
  - Useful for organizational reporting requirements
  - Integrates with external systems like SiServer

## Report Components

The monthly summary includes:

1. **Daily Breakdown**: Each workday with hours and productivity percentage
2. **Rest Days**: Company holidays and weekends with default hours
3. **Monthly Totals**: Total hours worked and average daily hours
4. **Productivity Metrics**: Average productivity across all workdays

## Data Sources

The summary integrates data from multiple sources:

- **Local Database**: Recorded workdays and pause information
- **External API**: Company rest dates and holidays (if configured)
- **Configuration**: Default work hours for rest days

## Productivity Calculation

Productivity is calculated as:
```
Productivity = (Net Working Time / Gross Working Time) * 100%
```

Where:
- **Net Working Time**: Actual productive work (excluding all pauses)
- **Gross Working Time**: Total presence time (excluding long breaks only)

This provides insight into how effectively time is used during work sessions.

## Rest Day Integration

If SiServer integration is configured, the summary will:
- Fetch official company rest dates for the current month
- Include these dates with default 8-hour entries
- Distinguish between work days and rest days in the display

## Examples

### Basic Monthly Summary

```bash
# Display monthly summary for current month
kasl sum
```

### Submit Monthly Report

```bash
# Generate and submit summary to configured API
kasl sum --send
```

## Sample Output

```
December 2024

+------------+----------+-------------+
| DATE       | DURATION | PRODUCTIVITY|
+------------+----------+-------------+
| 2024-12-02 | 08:15    | 92.0%       |
| 2024-12-03 | 07:45    | 88.0%       |
| 2024-12-04 | 08:30    | 95.0%       |
| 2024-12-05 | 07:20    | 85.0%       |
| 2024-12-06 | 08:00    | 90.0%       |
| 2024-12-07 | 08:00    | 100.0%      |
| 2024-12-08 | 08:00    | 100.0%      |
| 2024-12-09 | 08:15    | 91.0%       |
| 2024-12-10 | 07:30    | 87.0%       |
|            |          |             |
| TOTAL      | 79:15    | 92.5%       |
+------------+----------+-------------+
```

## Configuration

### SiServer Integration

To enable rest day integration, configure SiServer in your settings:

```bash
kasl init --si-server-url "https://your-server.com" --si-server-token "your-token"
```

### Default Work Hours

Configure default hours for rest days:

```bash
# Set default hours for rest days (default is 8 hours)
kasl init --default-hours 8
```

## Integration with Other Commands

The `sum` command works with other kasl commands:

- **`report`**: Daily reports that feed into monthly summaries
- **`watch`**: Activity monitoring that provides the underlying data
- **`adjust`**: Manual time adjustments that affect summary calculations
- **`export`**: Export monthly data for external analysis

## Use Cases

### Monthly Reporting

```bash
# Generate monthly report for management
kasl sum --send
```

### Productivity Analysis

```bash
# Review productivity trends
kasl sum
# Compare with previous months
```

### Compliance Reporting

```bash
# Submit required monthly reports
kasl sum --send
```

## Troubleshooting

### Common Issues

**No data for current month**
```bash
# Check if monitoring is running
kasl watch --foreground

# Verify data exists
kasl report --last
```

**API submission fails**
```bash
# Check API configuration
kasl init --show-config

# Test API connection
kasl report --send
```

**Incorrect rest days**
```bash
# Verify SiServer configuration
kasl init --si-server-url "https://your-server.com"

# Check rest day settings
kasl init --show-config
```

### Data Validation

```bash
# Verify daily data before generating summary
kasl report --last

# Check for missing days
kasl report --show-all
```

## Best Practices

### Monthly Workflow

1. **Review daily reports** throughout the month
2. **Generate summary** at month-end
3. **Submit to organization** if required
4. **Archive data** for historical analysis

### Data Quality

1. **Ensure monitoring is active** during work hours
2. **Review and adjust** any incorrect time entries
3. **Validate rest day configuration**
4. **Check API integration** before submission

### Reporting

1. **Submit reports promptly** at month-end
2. **Keep local copies** of submitted reports
3. **Review productivity trends** over time
4. **Use insights** to improve work patterns

## Related Commands

- **[`report`](./report.md)** - Generate daily work reports
- **[`watch`](./watch.md)** - Monitor activity for data collection
- **[`adjust`](./adjust.md)** - Adjust recorded times
- **[`export`](./export.md)** - Export data for external analysis
- **[`init`](./init.md)** - Configure API integration
