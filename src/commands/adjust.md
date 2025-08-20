# `adjust` Command

The `adjust` command provides manual correction capabilities for automatically recorded work times. It supports removing time from the beginning or end of workdays and adding manual pauses that weren't detected by the monitoring system.

## Usage

```bash
kasl adjust [OPTIONS]
```

## Options

- `-d, --date <DATE>`: Date to adjust workday for (default: `today`)
  - `today`: Current date
  - `YYYY-MM-DD`: Specific date in ISO format

- `-m, --minutes <MINUTES>`: Minutes to subtract or pause duration to add
  - **Start/End modes**: Minutes to remove from workday
  - **Pause mode**: Duration of the pause to add
  - If not specified, the user will be prompted interactively

- `--mode <MODE>`: Type of adjustment to perform
  - `start`: Remove time from the beginning of the workday
  - `end`: Remove time from the end of the workday
  - `pause`: Add a manual pause within the workday
  - If not provided, the user will be prompted to select

- `--force`: Skip confirmation prompt and apply changes immediately
  - Changes will be applied without showing a preview
  - Use with caution as adjustments modify the permanent work record

## Adjustment Modes

### Start Mode
Remove time from the beginning of the workday:

```bash
# Remove 30 minutes from the start of today's workday
kasl adjust --mode start --minutes 30

# Remove 15 minutes from a specific date
kasl adjust --mode start --minutes 15 --date 2024-12-15
```

**Use Cases:**
- Correcting early false starts from the monitor
- Accounting for personal time before actual work began
- Adjusting for system wake-up activity

### End Mode
Remove time from the end of the workday:

```bash
# Remove 20 minutes from the end of today's workday
kasl adjust --mode end --minutes 20

# Remove 45 minutes from a specific date
kasl adjust --mode end --minutes 45 --date 2024-12-15
```

**Use Cases:**
- Correcting late false activity from system processes
- Removing personal time after work ended
- Adjusting for shutdown activity

### Pause Mode
Add a manual pause in the middle of the workday:

```bash
# Add a 30-minute pause to today's workday
kasl adjust --mode pause --minutes 30

# Add a 60-minute pause to a specific date
kasl adjust --mode pause --minutes 60 --date 2024-12-15
```

**Use Cases:**
- Adding breaks that weren't automatically detected
- Recording meetings or phone calls away from the computer
- Accounting for manual activities not detected by monitoring

## Examples

### Interactive Mode

```bash
# Start interactive adjustment
kasl adjust

# The command will prompt for:
# 1. Adjustment mode (start/end/pause)
# 2. Duration in minutes
# 3. Confirmation of changes
```

### Non-Interactive Mode

```bash
# Remove 30 minutes from start of today's workday
kasl adjust --mode start --minutes 30

# Add 45-minute pause to yesterday's workday
kasl adjust --mode pause --minutes 45 --date 2024-12-15

# Remove 20 minutes from end of specific date
kasl adjust --mode end --minutes 20 --date 2024-12-10
```

### Force Mode

```bash
# Apply changes without confirmation
kasl adjust --mode start --minutes 30 --force

# Useful for automated scripts
kasl adjust --mode pause --minutes 15 --date 2024-12-15 --force
```

## Sample Output

### Interactive Session
```
Adjusting workday for 2024-12-15

Current workday:
├── Start: 09:00:00
├── End: 17:30:00
├── Duration: 8h 30m
└── Pauses: 1h 15m

Select adjustment mode:
1. Start (remove time from beginning)
2. End (remove time from end)
3. Pause (add manual pause)

Enter minutes: 30

Proposed changes:
├── Original duration: 8h 30m
├── Adjustment: -30 minutes
└── New duration: 8h 00m

Apply changes? [y/N]: y

✅ Workday adjusted successfully!
```

### Non-Interactive Output
```
Adjusting workday for 2024-12-15

Current workday:
├── Start: 09:00:00
├── End: 17:30:00
├── Duration: 8h 30m
└── Pauses: 1h 15m

Removing 30 minutes from start...

✅ Workday adjusted successfully!
New duration: 8h 00m
```

## Use Cases

### Correcting False Starts

```bash
# Remove early morning activity that wasn't actual work
kasl adjust --mode start --minutes 45 --date 2024-12-15
```

### Adding Missed Breaks

```bash
# Add lunch break that wasn't detected
kasl adjust --mode pause --minutes 60 --date 2024-12-15
```

### Correcting End Times

```bash
# Remove time spent on personal activities after work
kasl adjust --mode end --minutes 30 --date 2024-12-15
```

### Batch Adjustments

```bash
# Adjust multiple days (using scripts)
for date in 2024-12-15 2024-12-16 2024-12-17; do
    kasl adjust --mode start --minutes 30 --date $date --force
done
```

## Validation and Safety

### Pre-Adjustment Checks

The command performs several validations:

1. **Workday Existence**: Ensures a workday exists for the specified date
2. **Time Validation**: Checks that adjustments don't create invalid time ranges
3. **Duration Limits**: Prevents adjustments that would result in negative work time
4. **Pause Overlap**: Validates pause times don't conflict with existing pauses

### Confirmation Process

Unless `--force` is used, the command will:

1. **Show Current State**: Display the current workday information
2. **Preview Changes**: Show what the adjustment will do
3. **Request Confirmation**: Ask for user approval before applying changes

## Troubleshooting

### Common Issues

**No workday found for date**
```bash
# Check if workday exists
kasl report --date 2024-12-15

# Create workday if needed
kasl watch --foreground
```

**Invalid adjustment amount**
```bash
# Check current workday duration
kasl report --date 2024-12-15

# Use smaller adjustment amount
kasl adjust --mode start --minutes 15
```

**Pause time conflicts**
```bash
# View existing pauses
kasl pauses --date 2024-12-15

# Choose different pause time
kasl adjust --mode pause --minutes 30
```

### Data Recovery

```bash
# View workday history
kasl report --date 2024-12-15

# Export data before making changes
kasl export --date 2024-12-15 --output backup.csv
```

## Best Practices

### Before Making Adjustments

1. **Review current data** using `report` command
2. **Export backup** using `export` command
3. **Understand the impact** of the adjustment
4. **Test with small amounts** first

### Adjustment Strategy

1. **Use specific dates** rather than 'today' for historical adjustments
2. **Make incremental changes** rather than large adjustments
3. **Document reasons** for adjustments
4. **Verify results** after making changes

### Data Integrity

1. **Avoid excessive adjustments** that might indicate monitoring issues
2. **Review patterns** in adjustments to improve monitoring
3. **Keep adjustments minimal** and well-documented
4. **Validate final results** with report command

## Integration with Other Commands

The `adjust` command works with other kasl commands:

- **`report`**: Review workday data before and after adjustments
- **`pauses`**: View existing pauses to avoid conflicts
- **`watch`**: Improve monitoring to reduce need for adjustments
- **`export`**: Backup data before making adjustments

## Related Commands

- **[`report`](./report.md)** - Review workday data
- **[`pauses`](./pauses.md)** - View recorded breaks and pauses
- **[`watch`](./watch.md)** - Monitor activity (reduces need for adjustments)
- **[`export`](./export.md)** - Backup data before adjustments
