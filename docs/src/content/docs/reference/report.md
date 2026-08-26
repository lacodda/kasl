---
title: "report"
---

The `report` command builds the daily report from recorded intervals, pauses and tasks, and can submit it - or a monthly summary - to the configured API.

## Usage

```bash
kasl report [OPTIONS]
```

## Options

- `--send`: Submit the generated daily report to the configured API (typically SiServer), in addition to displaying it.
- `-l, --last`: Generate the report for the previous day instead of today.
- `--month`: Submit a monthly summary report to the configured API instead of a daily one.

## Examples

```bash
# Generate and display today's report
kasl report

# Generate and send today's report
kasl report --send

# Generate report for yesterday
kasl report --last

# Generate yesterday's report and send it
kasl report --last --send

# Submit the monthly summary report
kasl report --month
```

## Short interval filtering

Work intervals shorter than the configured `min_work_interval` are left out of the table - at display time only, so nothing in the database changes - and the same filtering applies whether the report is shown locally or sent with `--send`. When intervals are filtered, the report says how many and their total duration.

## Sample Output

```
August 8, 2026

+--------------+-------+-------+----------+
| ID           | START | END   | DURATION |
+--------------+-------+-------+----------+
| 1            | 09:12 | 13:30 | 04:18    |
| 2            | 14:18 | 16:02 | 01:44    |
| 3            | 16:29 | 18:04 | 01:35    |
|              |       |       |          |
| TOTAL        |       |       | 07:37    |
| PRODUCTIVITY |       |       | 96.1%    |
+--------------+-------+-------+----------+

Tasks:

+---+----+---------------------------------------------------+------------------+------+
| # | ID | NAME                                              | COMMENT          | DONE |
+---+----+---------------------------------------------------+------------------+------+
| 1 | 1  | PROJ-412 Fix session timeout on the settings page | stale cookie jar | 100% |
| 2 | 2  | Review PR #318: pause merging                     |                  | 100% |
| 3 | 3  | PROJ-419 Draft migration for protected pauses     | backfill pending | 60%  |
+---+----+---------------------------------------------------+------------------+------+
```

Sending it:

```
$ kasl report --send
Your report dated August 8, 2026 has been successfully submitted
Wait for a message to your email address
```

## Related commands

- [`watch`](/reference/watch/) - Monitor activity for report data
- [`task`](/reference/task/) - Manage tasks included in reports
- [`pauses`](/reference/pauses/) - Record absences reflected in reports
- [`sum`](/reference/sum/) - Generate monthly summaries
