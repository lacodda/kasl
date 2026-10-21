---
title: "sum"
---

The `sum` command shows the current month's working hours: one row per recorded day, plus totals and an average.

## Usage

```bash
kasl sum [OPTIONS]
```

- `--send`: Submit the monthly summary to the configured reporting API, in addition to displaying it.
- `--json`: Print the month as one JSON document instead of the table - see [JSON output](#json-output). Not accepted with `--send`.

## Examples

```bash
# Display this month's summary
kasl sum

# Generate and submit the monthly report
kasl sum --send
```

## Sample Output

```
$ kasl sum
Working hours for August, 2026

+------------+-------+--------------+
| DATE       | HOURS | PRODUCTIVITY |
+------------+-------+--------------+
| 2026-08-07 | 08:04 | 94.7%        |
| 2026-08-08 | 07:37 | 96.1%        |
|            |       |              |
| TOTAL      | 15:41 |              |
| AVERAGE    | 07:50 |              |
+------------+-------+--------------+

Monthly work productivity: 95.4%
```

Each day's hours are the TOTAL its [`report`](/reference/report/) shows: the
intervals between long pauses, with the ones shorter than `min_work_interval`
left out. A day reads the same in the report and in the month.

## JSON output

`kasl sum --json` prints the same month as one document (conventions:
[kasl in scripts and CI](/guides/scripting/#json-for-programs)):

```json
{
  "month": "2026-08",
  "days": [
    { "date": "2026-08-07", "work_seconds": 29040, "productivity": 94.7, "rest_day": false },
    { "date": "2026-08-08", "work_seconds": 27420, "productivity": 96.1, "rest_day": false }
  ],
  "total_seconds": 56460,
  "average_seconds": 28230,
  "productivity": 95.4
}
```

`rest_day` marks a company rest day the reporting API named, which the month
counts as a full eight-hour day without anyone having worked it. The closing
`productivity` is the average over recorded days; it is `null` while there are
none.

## Related commands

- [`report`](/reference/report/) - Generate a single day's report
- [`watch`](/reference/watch/) - Monitor activity that feeds into the summary
- [`pauses`](/reference/pauses/) - Record absences that affect productivity
- [`export`](/reference/export/) - Export monthly summary data
- [`setup`](/reference/setup/) - Configure the reporting API
