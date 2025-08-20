
# `report` Command

The `report` command generates comprehensive daily work reports with detailed information about work sessions, breaks, productivity metrics, and completed tasks. It also supports automatic report submission to external systems and monthly summary generation.

## Usage

```bash
kasl report [OPTIONS]
```

## Options

- `--send`: Submit the generated daily report to configured API
  - Automatically submits the report to the configured reporting service (typically SiServer)
  - Enables integration with organizational time tracking systems
  - Useful for compliance and organizational reporting requirements

- `--last`, `-l`: Generate report for the previous day instead of today
  - Useful for submitting yesterday's report in the morning
  - Reviewing completed work sessions
  - Batch processing of historical reports

- `--month`: Submit monthly summary report to configured API
  - Generates and submits an aggregate monthly report
  - Contains summary statistics and total work hours
  - Typically used for organizational reporting requirements at month-end

- `--clear-short-intervals`, `-c`: Automatically detect and remove short work intervals
  - Analyzes work intervals and removes pauses that create inappropriately short work periods
  - Merges adjacent intervals for cleaner reporting
  - Helps eliminate noise from brief interruptions

## Examples

### Basic Report Generation

```bash
# Generate and display today's report
kasl report

# Generate report for yesterday
kasl report --last

# Generate and send today's report
kasl report --send
```

### Advanced Reporting

```bash
# Submit monthly summary report
kasl report --month

# Clean up short intervals and generate report
kasl report --clear-short-intervals

# Generate yesterday's report and send it
kasl report --last --send
```

### Report Analysis

```bash
# Review today's work without sending
kasl report

# Check yesterday's productivity
kasl report --last

# Clean up data and review
kasl report --clear-short-intervals
```

## Report Components

### Daily Report Structure

The daily report includes:

1. **Work Session Summary**
   - Start and end times
   - Total work duration
   - Productivity percentage
   - Break periods

2. **Task Completion**
   - List of completed tasks
   - Task completion percentages
   - Task descriptions and comments

3. **Productivity Metrics**
   - Net working time vs. gross time
   - Break analysis
   - Efficiency calculations

4. **Time Intervals**
   - Detailed work periods
   - Break periods
   - Activity patterns

### Monthly Report Structure

Monthly reports include:

1. **Aggregate Statistics**
   - Total work hours for the month
   - Average daily hours
   - Overall productivity percentage

2. **Daily Breakdown**
   - Each workday with hours and productivity
   - Rest days with default hours
   - Missing days identification

3. **Trend Analysis**
   - Productivity trends
   - Work pattern analysis
   - Performance insights

## Sample Output

### Daily Report
```
December 15, 2024

+--------------+-------+-------+----------+
| ID           | START | END   | DURATION |
+--------------+-------+-------+----------+
| 1            | 09:00 | 12:00 | 03:00    |
| 2            | 13:00 | 15:30 | 02:30    |
| 3            | 15:45 | 17:30 | 01:45    |
|              |       |       |          |
| TOTAL        |       |       | 07:15    |
| PRODUCTIVITY |       |       | 92.0%    |
+--------------+-------+-------+----------+

Tasks

+---+----+----------+------------------+------------------+-------------+------------------+
| # | ID | TASK ID | NAME             | COMMENT          | COMPLETENESS| TAGS             |
+---+----+----------+------------------+------------------+-------------+------------------+
| 1 | 1  | 0       | Daily standup    | Team sync        | 100%        | meeting          |
| 2 | 2  | 0       | Code review      | Review PR #123   | 100%        | urgent           |
| 3 | 3  | 0       | Bug fix          | Fix login issue  | 75%         | bug, urgent      |
| 4 | 4  | 0       | Documentation    | Update API docs  | 25%         | docs             |
+---+----+----------+------------------+------------------+-------------+------------------+
```

### Monthly Report
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
|            |          |             |
| TOTAL      | 39:50    | 90.0%       |
+------------+----------+-------------+
```

## Use Cases

### Daily Work Review

```bash
# Review today's work at end of day
kasl report

# Submit report to organization
kasl report --send
```

### Weekly Planning

```bash
# Review yesterday's work before planning today
kasl report --last

# Clean up data for better analysis
kasl report --clear-short-intervals
```

### Monthly Reporting

```bash
# Generate monthly summary for management
kasl report --month

# Submit monthly report to HR system
kasl report --month --send
```

### Data Quality Management

```bash
# Clean up short intervals for better reporting
kasl report --clear-short-intervals

# Review cleaned data
kasl report
```

## Integration with Other Commands

The `report` command works with other kasl commands:

- **`watch`**: Uses data collected by activity monitoring
- **`task`**: Includes task completion data in reports
- **`adjust`**: Reflects manual time adjustments in reports
- **`pauses`**: Integrates pause data for comprehensive reporting

## Best Practices

### Daily Workflow

1. **Review at end of day**: Generate report to review productivity
2. **Submit promptly**: Send reports to maintain compliance
3. **Clean data regularly**: Use short interval cleanup for accuracy
4. **Monitor trends**: Track productivity patterns over time

### Report Quality

1. **Verify data accuracy**: Check reports for unusual patterns
2. **Clean up noise**: Remove short intervals that create confusion
3. **Document adjustments**: Note any manual time corrections
4. **Review completeness**: Ensure all work is properly recorded

### Organizational Integration

1. **Configure API settings**: Set up proper integration with reporting systems
2. **Test submissions**: Verify report delivery before relying on automation
3. **Monitor compliance**: Ensure reports meet organizational requirements
4. **Backup locally**: Keep local copies of submitted reports

## Related Commands

- **[`watch`](./watch.md)** - Monitor activity for report data
- **[`task`](./task.md)** - Manage tasks included in reports
- **[`adjust`](./adjust.md)** - Adjust times reflected in reports
- **[`pauses`](./pauses.md)** - View breaks included in reports
- **[`sum`](./sum.md)** - Generate monthly summaries
