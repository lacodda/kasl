# `breaks` Command

The `breaks` command enables strategic placement of manual break periods to optimize productivity metrics and meet minimum thresholds for report submission. This productivity-focused feature helps users improve their work reports by adding intentional breaks that complement automatic pause detection.

## Usage

```bash
kasl breaks [OPTIONS]
```

## Options

- `-m, --minutes <MINUTES>`: Duration of the break in minutes
  - When specified, automatically finds optimal placement for the break
  - If not specified, enters interactive mode for manual break configuration
  - Must be within configured minimum and maximum break duration limits

- `--force`: Force creation even if productivity validation fails
  - Bypasses normal productivity threshold checks
  - Creates the break regardless of current productivity levels
  - Use with caution as this skips validation safeguards

## Break Management Modes

### Automatic Mode
Optimal break placement with specified duration:

```bash
# Add a 30-minute break with automatic optimal placement
kasl breaks -m 30

# Add a 60-minute break, bypassing productivity checks
kasl breaks -m 60 --force
```

**How Automatic Placement Works:**
- Analyzes existing work intervals and pauses
- Finds optimal placement using multiple strategies
- Avoids conflicts with existing pauses
- Maintains minimum work intervals around breaks

### Interactive Mode
Guided break creation with user selection:

```bash
# Start interactive break creation
kasl breaks

# The command will prompt for:
# 1. Break duration (within configured limits)
# 2. Placement options with timing details
# 3. Selection from available placement strategies
```

## Break Placement Strategies

The system uses intelligent algorithms to suggest optimal break placement:

### 1. Middle of Longest Work Period
Places the break in the center of the longest uninterrupted work interval:
- **Advantage**: Splits long work sessions effectively
- **Best for**: Improving focus and preventing fatigue

### 2. After Existing Pauses
Places breaks immediately following detected pauses:
- **Advantage**: Extends natural break periods
- **Best for**: Consolidating rest time

### 3. Before Existing Pauses
Places breaks just before detected pauses:
- **Advantage**: Creates preparation time before natural breaks
- **Best for**: Structuring work transitions

## Examples

### Quick Break Addition

```bash
# Add 30-minute break with optimal placement
kasl breaks -m 30

# Output:
# ✅ Break created: 14:30 - 15:00 (30 minutes)
# 📈 Productivity improved from 68% to 76%
```

### Interactive Break Creation

```bash
# Start interactive mode
kasl breaks

# Sample interaction:
# Enter break duration (20-180 minutes): 45
# 
# Available break placement options:
# 1. 12:15 - 13:00 (45 min) - Middle of longest work period
# 2. 10:30 - 11:15 (45 min) - After morning pause
# 3. 15:45 - 16:30 (45 min) - Before afternoon pause
# 
# Select break placement: 1
# 
# ✅ Break created: 12:15 - 13:00 (45 minutes)
```

### Productivity Optimization

```bash
# When productivity is too low for report submission:
# ⚠️ Current productivity: 68% (below 75% threshold)
# 💡 Add a 22-minute break to reach minimum productivity

kasl breaks -m 22

# ✅ Break created: 13:00 - 13:22 (22 minutes)
# 📈 Productivity improved to 75% - report can now be submitted
```

## Productivity Integration

### Automatic Recommendations

The system provides productivity recommendations when:
- Current productivity is below configured threshold (default: 75%)
- Enough of the workday has passed to make meaningful suggestions
- Report submission is blocked due to low productivity

### Threshold Validation

Before creating breaks, the system:
1. **Calculates current productivity** including existing pauses and breaks
2. **Validates duration limits** based on configuration
3. **Checks for conflicts** with existing pauses
4. **Ensures minimum work intervals** are maintained

### Report Integration

Breaks directly impact report functionality:
- **Report display** includes break information and productivity metrics
- **API submission** validates productivity before sending reports
- **Productivity warnings** suggest break creation when needed

## Configuration

Break behavior is controlled by configuration settings:

```json
{
  "productivity": {
    "min_productivity_threshold": 75.0,
    "workday_hours": 8.0,
    "min_break_duration": 20,
    "max_break_duration": 180,
    "min_workday_fraction_before_suggest": 0.5
  }
}
```

### Configuration Options

- **`min_productivity_threshold`**: Minimum productivity percentage for report submission
- **`workday_hours`**: Expected daily work hours for calculations
- **`min_break_duration`**: Minimum allowed break duration in minutes
- **`max_break_duration`**: Maximum allowed break duration in minutes
- **`min_workday_fraction_before_suggest`**: Fraction of workday before showing recommendations

## Sample Output

### Successful Break Creation
```
🔄 Analyzing workday and existing pauses...
✅ Break created: 14:30 - 15:00 (30 minutes)
📈 Productivity updated: 68% → 76%
```

### Interactive Selection
```
📋 Break duration: 45 minutes

Available placement options:
1. 12:15 - 13:00 (45 min) - Middle of longest work period
2. 10:30 - 11:15 (45 min) - After 15-minute pause
3. 15:45 - 16:30 (45 min) - Before 8-minute pause

Select break placement: 2

✅ Break created: 10:30 - 11:15 (45 minutes)
📈 Productivity improved from 68% to 75%
```

### Productivity Warning
```
⚠️ Low productivity detected: 68% (threshold: 75%)
💡 Consider adding a 22-minute break to reach minimum productivity
📝 Use: kasl breaks -m 22
```

## Use Cases

### Meeting Productivity Thresholds

```bash
# When report submission is blocked:
kasl report
# ❌ Productivity too low (68%) - minimum 75% required
# 💡 Add 22-minute break to reach threshold

kasl breaks -m 22
# ✅ Break added - productivity now 75%

kasl report
# ✅ Report sent successfully
```

### Improving Work-Life Balance

```bash
# Add structured lunch break
kasl breaks -m 60

# Add afternoon rest period
kasl breaks -m 30
```

### Optimizing Long Work Sessions

```bash
# Add break in middle of long work period
kasl breaks
# Select: "Middle of longest work period"
```

## Validation and Safety

### Pre-Creation Checks

The command performs several validations:

1. **Workday Existence**: Ensures a workday exists for today
2. **Duration Limits**: Validates break duration within configured bounds
3. **Conflict Detection**: Prevents overlaps with existing pauses
4. **Work Interval Maintenance**: Ensures minimum work time around breaks

### Smart Placement

The placement algorithm ensures:

1. **No Overlaps**: Breaks don't conflict with existing pauses
2. **Minimum Work Time**: Maintains required work intervals
3. **Optimal Timing**: Places breaks for maximum productivity benefit
4. **Current Time Validation**: Only places breaks in completed time periods

## Troubleshooting

### Common Issues

**No valid break placement found**
```bash
# Check existing pauses and work intervals
kasl pauses

# Try shorter break duration
kasl breaks -m 20

# Check if enough work time has passed
kasl report
```

**Break duration out of range**
```bash
# Check configured limits
# Default: 20-180 minutes

# Use duration within limits
kasl breaks -m 45
```

**Productivity still too low after break**
```bash
# Check current productivity
kasl report

# Add additional break if needed
kasl breaks -m 30
```

### Data Validation

```bash
# View current workday status
kasl report

# Check existing pauses
kasl pauses

# Review productivity metrics
kasl report --productivity
```

## Best Practices

### Strategic Break Placement

1. **Target productivity thresholds** rather than arbitrary break times
2. **Use automatic mode** for optimal placement
3. **Add breaks gradually** to see productivity impact
4. **Monitor report feedback** to understand effectiveness

### Productivity Optimization

1. **Check productivity early** in the workday
2. **Add breaks proactively** before productivity drops too low
3. **Use recommended durations** from productivity warnings
4. **Balance break time** with actual work requirements

### Integration Workflow

1. **Monitor productivity** throughout the day
2. **Add breaks when warned** by the system
3. **Validate report submission** after adding breaks
4. **Review daily patterns** to improve future planning

## Integration with Other Commands

The `breaks` command works seamlessly with other kasl commands:

- **`report`**: Shows productivity metrics and break impact
- **`pauses`**: Displays existing pauses to avoid conflicts
- **`watch`**: Continues monitoring activity with breaks included
- **`export`**: Includes break data in exported reports

## Related Commands

- **[`report`](./report.md)** - View productivity metrics and break impact
- **[`pauses`](./pauses.md)** - View existing pauses and gaps
- **[`watch`](./watch.md)** - Monitor activity including break periods
- **[`export`](./export.md)** - Export data including break information