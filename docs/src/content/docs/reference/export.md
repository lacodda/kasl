---
title: "export"
---

The `export` command writes report, task, or summary data to a file - CSV, JSON, or Excel - for backup or use outside kasl.

## Usage

```bash
kasl export [DATA] [OPTIONS]
```

## Arguments

- `DATA` (default: `report`):
  - `report`: The daily report - intervals, tasks, productivity.
  - `tasks`: The date's tasks.
  - `summary`: The month's totals and per-day hours.
  - `all`: Report + tasks + summary - one JSON file, or suffixed files for CSV/Excel.

## Options

- `-f, --format <FORMAT>` (default: `csv`): `csv`, `json` (pretty-printed), or `excel` (one worksheet per export, headers and autofit applied).
- `-o, --output <PATH>`: Custom output file path. If omitted, a name is generated from timestamp, data type, and format: `kasl_export_20250115_143022.csv`.
- `-d, --date <DATE>` (default: `today`): `today` or `YYYY-MM-DD`. For `summary`, this picks the month; for `report`/`tasks`, the exact date.
- `--hourly`: Render the daily report as an hourly (SiServer-style) grid instead of a list of intervals - one row per hour of the workday, each with a description of the work performed. Hours (or parts of hours) that fall inside a break or pause get a localized break label instead - "Break" in English, or the Russian equivalent when `report.language = "ru"` is set in the config. This flag only affects `export report --format excel`; it is ignored for every other data type/format combination.

## Examples

```bash
# Export today's report as CSV (the default)
kasl export

# Export a specific date's report as Excel
kasl export report --date 2026-08-15 --format excel

# Export today's report as an hourly Excel grid
kasl export report --format excel --hourly

# Export all tasks as JSON
kasl export tasks --format json

# Export the current month's summary as Excel
kasl export summary --format excel

# Export everything to one JSON file
kasl export all --format json --output backup.json
```

## Sample Output

A CSV report export is written in three sections - intervals, summary, tasks - separated by blank rows, so one file holds the whole day:

```csv
WORK INTERVALS,,,
Index,Start,End,Duration
1,09:12,13:30,04:18
2,14:18,16:02,01:44
3,16:29,18:04,01:35
,,,
SUMMARY,,,
Date,2026-08-08,,
Total Hours,07:37,,
Productivity,96.1%,,
,,,
TASKS,,,
ID,Name,Comment,Completeness
1,PROJ-412 Fix session timeout on the settings page,stale cookie jar,100%
```

A JSON tasks export is a plain array, no wrapper object:

```json
[
  {
    "id": 1,
    "name": "PROJ-412 Fix session timeout on the settings page",
    "comment": "stale cookie jar",
    "completeness": 100
  }
]
```

An Excel export writes a single worksheet holding the same rows the CSV export would contain. No charts are generated.

## Related commands

- [`report`](/reference/report/) - Generate a daily report
- [`sum`](/reference/sum/) - Generate a monthly summary
- [`task`](/reference/task/) - Manage tasks
- [`watch`](/reference/watch/) - Monitor activity that feeds exports
