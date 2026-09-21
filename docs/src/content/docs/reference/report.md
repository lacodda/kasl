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
- `--show`: Print the payload `--send` would post, and post nothing. Requires `--send`; not accepted with `--month`.

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

# See exactly what --send would post, without posting it
kasl report --send --show
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

When the Jira inbox holds anything, the report closes with a line about what is
still waiting - the day is not only what got done:

```
3 in the inbox (1 new, 1 taken)
```

The counts leave out dismissed issues and issues that left Jira: they name what
still asks for attention. Nothing is printed when the inbox is empty or not
configured. See [`inbox`](/reference/inbox/).

Sending it:

```
$ kasl report --send
Your report dated August 8, 2026 has been successfully submitted
Wait for a message to your email address
```

## What gets sent

`--send` posts a multipart form to the corporate API, and `--show` prints that
form instead of posting it:

```console
$ kasl report --send --show
This is what 2026-08-08 would send to https://api.example.com/report-card/send-daily-report, as a multipart form:
  date: 2026-08-08
  tasks:
[
  {
    "from": "09:12",
    "index": 1,
    "result": "",
    "task": "PROJ-412 Fix session timeout on the settings page (100%)",
    "time": "",
    "to": "13:30",
    "total_ts": "04:18"
  }
]
  comment: (empty)
  day_type: 1
  duty: 0
  only_save: 0
```

Every field of the request is listed, including the ones sent empty: a field you
were not shown is a field you were not told about. `date`, `tasks` and the three
constants are the whole form - `day_type` 1 is a working day, `duty` 0 is not on
call, and `only_save` 0 means submit rather than keep as a draft.

The preview is built by the same code that builds the request, so the two cannot
drift apart. It stops one step short of sending, and three things `--send` does
are deliberately left undone:

- **The day is not finalized.** `--send` writes an end timestamp before it
  assembles anything; a preview that did the same would end your working day for
  asking what sending would look like.
- **The productivity threshold is not applied.** It decides whether a report may
  be submitted, not what the submission contains, and a day below it is exactly
  the day worth inspecting.
- **Nothing is authenticated.** No session is opened and no credential is read.
  The address comes from your config, so this works offline.

For the other channel - the team server - the same question is answered by
[`kasl server manifest`](/reference/server/#kasl-server-manifest), which reads
what that installation stores from the server itself.

## Related commands

- [`watch`](/reference/watch/) - Monitor activity for report data
- [`task`](/reference/task/) - Manage tasks included in reports
- [`pauses`](/reference/pauses/) - Record absences reflected in reports
- [`sum`](/reference/sum/) - Generate monthly summaries
- [`inbox`](/reference/inbox/) - The issues counted at the end of the report
- [`server`](/reference/server/) - The team server, and what it stores about you
