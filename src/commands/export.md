# `export` Command

The `export` command provides comprehensive data export functionality, supporting multiple output formats and data types. It enables users to extract their work data for external analysis, backup purposes, or integration with other tools and systems.

## Usage

```bash
kasl export [DATA_TYPE] [OPTIONS]
```

## Arguments

- `DATA_TYPE`: Type of data to export (default: `report`)
  - `report`: Daily work report with intervals and productivity
  - `tasks`: Task records with completion status and metadata
  - `summary`: Monthly summary with aggregate statistics
  - `all`: Complete data export including all available information

## Options

- `-f, --format <FORMAT>`: Output format for the exported data (default: `csv`)
  - `csv`: Comma-separated values, compatible with Excel and other spreadsheet tools
  - `json`: Structured JSON data, ideal for programmatic processing
  - `excel`: Native Excel format with formatting, charts, and multiple worksheets

- `-o, --output <PATH>`: Custom output file path
  - When specified, the export will be saved to this exact location
  - If not provided, a default filename will be generated based on timestamp, data type, and format

- `-d, --date <DATE>`: Target date for data export (default: `today`)
  - `today`: Current date
  - `YYYY-MM-DD`: Specific date in ISO format
  - For summary exports, this determines the month to summarize

## Supported Export Formats

### CSV Format
- **Compatibility**: Works with Excel, Google Sheets, and other spreadsheet applications
- **Structure**: Tabular data with headers
- **Use Cases**: Quick analysis, data sharing, simple reporting

### JSON Format
- **Compatibility**: Ideal for programmatic processing and API integration
- **Structure**: Hierarchical data with metadata
- **Use Cases**: Custom analysis scripts, data integration, backup

### Excel Format
- **Compatibility**: Native Excel with formatting and multiple sheets
- **Structure**: Multiple worksheets with charts and formatting
- **Use Cases**: Professional reporting, complex analysis, presentation

## Data Types

### Report Export
Exports daily work reports with intervals, tasks, and productivity metrics:

```bash
# Export today's report as CSV
kasl export report --format csv

# Export yesterday's report as Excel
kasl export report --date 2024-12-15 --format excel
```

### Tasks Export
Exports task records with completion status and metadata:

```bash
# Export all tasks as JSON
kasl export tasks --format json

# Export today's tasks as CSV
kasl export tasks --date today --format csv
```

### Summary Export
Exports monthly summaries with aggregate statistics:

```bash
# Export current month summary as Excel
kasl export summary --format excel

# Export specific month as CSV
kasl export summary --date 2024-11-01 --format csv
```

### Complete Export
Exports all available data for comprehensive backup:

```bash
# Export all data as JSON
kasl export all --format json

# Export all data as Excel with custom filename
kasl export all --format excel --output my_backup.xlsx
```

## Examples

### Basic Exports

```bash
# Export today's report (default)
kasl export

# Export today's report as CSV
kasl export report --format csv

# Export today's report as JSON
kasl export report --format json
```

### Custom File Names

```bash
# Export with custom filename
kasl export --output my_report.csv

# Export with date-specific filename
kasl export --date 2024-12-15 --output december_15_report.xlsx
```

### Different Data Types

```bash
# Export tasks
kasl export tasks --format csv

# Export monthly summary
kasl export summary --format excel

# Export complete backup
kasl export all --format json --output backup.json
```

### Date-Specific Exports

```bash
# Export yesterday's data
kasl export --date 2024-12-15

# Export specific date
kasl export --date 2024-11-30 --format excel
```

## Sample Output

### CSV Export (Report)
```csv
Date,Start Time,End Time,Duration,Productivity,Task Count
2024-12-15,09:00:00,17:30:00,8h 30m,92%,5
```

### JSON Export (Tasks)
```json
{
  "date": "2024-12-15",
  "tasks": [
    {
      "id": 1,
      "name": "Code review",
      "comment": "Review pull request #123",
      "completeness": 100,
      "tags": ["urgent", "backend"]
    }
  ]
}
```

### Excel Export (Summary)
- **Sheet 1**: Daily breakdown with charts
- **Sheet 2**: Task completion statistics
- **Sheet 3**: Productivity analysis
- **Sheet 4**: Monthly totals and trends

## File Naming Convention

Default filenames follow this pattern:
```
kasl_export_YYYYMMDD_HHMMSS.[format]
```

Examples:
- `kasl_export_20241215_143022.csv`
- `kasl_export_20241215_143022.json`
- `kasl_export_20241215_143022.xlsx`

## Use Cases

### Data Backup

```bash
# Create comprehensive backup
kasl export all --format json --output backup_$(date +%Y%m%d).json
```

### External Analysis

```bash
# Export for spreadsheet analysis
kasl export report --format csv --output analysis.csv

# Export for custom scripts
kasl export tasks --format json --output tasks.json
```

### Reporting

```bash
# Create professional report
kasl export summary --format excel --output monthly_report.xlsx

# Export for management review
kasl export report --format excel --output daily_report.xlsx
```

### Integration

```bash
# Export for API integration
kasl export all --format json --output api_data.json

# Export for database import
kasl export tasks --format csv --output tasks_import.csv
```

## Troubleshooting

### Common Issues

**File permission errors**
```bash
# Check directory permissions
ls -la /path/to/output/directory

# Use different output location
kasl export --output ~/Desktop/export.csv
```

**Format not supported**
```bash
# Check available formats
kasl export --help

# Use different format
kasl export --format csv
```

**Date parsing errors**
```bash
# Use correct date format
kasl export --date 2024-12-15

# Use 'today' for current date
kasl export --date today
```

### Data Validation

```bash
# Verify data exists before export
kasl report --date 2024-12-15

# Check file was created
ls -la kasl_export_*.csv
```

## Best Practices

### Regular Backups

1. **Schedule regular exports** for data backup
2. **Use different formats** for different purposes
3. **Store backups securely** in multiple locations
4. **Verify backup integrity** after creation

### File Management

1. **Use descriptive filenames** with dates
2. **Organize exports** in dedicated directories
3. **Archive old exports** to save space
4. **Document export purposes** for future reference

### Data Analysis

1. **Use CSV** for quick spreadsheet analysis
2. **Use JSON** for custom scripts and APIs
3. **Use Excel** for professional reporting
4. **Combine formats** for comprehensive analysis

## Integration with Other Commands

The `export` command works with other kasl commands:

- **`report`**: Export daily reports
- **`sum`**: Export monthly summaries
- **`task`**: Export task data
- **`watch`**: Data collected by monitoring feeds into exports

## Related Commands

- **[`report`](./report.md)** - Generate daily work reports
- **[`sum`](./sum.md)** - Generate monthly summaries
- **[`task`](./task.md)** - Manage tasks for export
- **[`watch`](./watch.md)** - Monitor activity for data collection
