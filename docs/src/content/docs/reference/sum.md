---
title: "sum"
---

The `sum` command shows the current month's working hours: one row per recorded day, plus totals and an average.

## Usage

```bash
kasl sum [OPTIONS]
```

- `--send`: Submit the monthly summary to the configured reporting API, in addition to displaying it.

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

## Related commands

- [`report`](/reference/report/) - Generate a single day's report
- [`watch`](/reference/watch/) - Monitor activity that feeds into the summary
- [`pauses`](/reference/pauses/) - Record absences that affect productivity
- [`export`](/reference/export/) - Export monthly summary data
- [`setup`](/reference/setup/) - Configure the reporting API
