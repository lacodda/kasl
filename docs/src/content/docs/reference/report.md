---
title: "report"
---

The `report` command builds the daily report from recorded intervals, pauses and tasks, and can submit it - or a monthly summary - to the configured API.

## Usage

```bash
kasl report [OPTIONS]
```

## Options

- `--send`: Close the day and send it to every channel it goes to: SiServer when it is configured, and every [plugin](/reference/plugin/) that hears `report_ready` - see [Sending](#sending).
- `-l, --last`: Generate the report for the previous day instead of today.
- `--month`: Submit a monthly summary report to the configured API instead of a daily one.
- `--show`: Print the payload `--send` would post, and post nothing. Requires `--send`; not accepted with `--month`.
- `--json`: Print the day as one JSON document instead of the table - see [JSON output](#json-output). With `--send --show`, the payload as JSON. Not accepted with `--month`, nor with `--send` alone: sending prints what happened to the request, not data.

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

## A day that was never closed

A day ends when `kasl end` or `kasl report --send` closes it. Closing today ends it now; closing a past day ends it at its last observed activity, by the rule below, and a past day already closed keeps its end - so sending yesterday this morning does not stretch yesterday into today. Today, an open day runs to now. A past day nobody closed runs to the last moment activity was seen: the end of its last pause, of any length. The end of a two-minute break is as good a sighting as the end of a lunch, so an evening worked between short breaks stays in the day. Productivity is measured to the same moment, and [`sum`](/reference/sum/) counts the day the same way.

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

When the inbox holds anything, the report closes with a line about what is
still waiting - the day is not only what got done:

```
3 in the inbox (1 new, 1 taken)
```

The counts leave out dismissed issues and issues that left their source: they name what
still asks for attention. Nothing is printed when the inbox is empty or not
configured. See [`inbox`](/reference/inbox/).

Sending it:

```
$ kasl report --send
Your report dated August 8, 2026 has been successfully submitted
Wait for a message to your email address
```

## Sending

`--send` closes the day, checks it, and hands it to every channel:

1. **Where it goes is found first.** SiServer when the config has it, and every
   plugin that is set up and hears `report_ready`. With neither, the command
   refuses before it writes anything - a send with nowhere to go does not close
   the day on its way to saying so.
2. **The day is closed**, by the rule [`end`](/reference/end/) follows: today
   ends now, a past day at its last activity, a closed past day keeps its end.
   An end newly written is a `workday_end` for the plugins that hear it.
3. **The day is checked.** It needs its tasks, and productivity over the
   configured minimum; a day that fails either is not sent anywhere.
4. **Each channel gets it on its own.** SiServer is posted the form below;
   each `report_ready` plugin is handed the day as `kasl report --json` prints
   it. One failing does not stop the other, and a plugin's failure is a
   warning naming it.

```
$ kasl report --send
Your report dated August 8, 2026 has been successfully submitted
[i] outlook: Mailed the day to me@example.com
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

The preview also names the plugins that would be handed the day:

```
[i] The day also goes to plugins that hear report_ready: outlook.
```

For the other channel - the team server - the same question is answered by
[`kasl server manifest`](/reference/server/#kasl-server-manifest), which reads
what that installation stores from the server itself.

`kasl report --send --show --json` prints the same payload as one document:
the address, the date, every field of the form in order and exactly as sent,
the `tasks` field once more, parsed, and the plugins that would hear
`report_ready`:

```json
{
  "url": "https://api.example.com/report-card/send-daily-report",
  "date": "2026-08-08",
  "fields": [
    { "name": "date", "value": "2026-08-08" },
    { "name": "tasks", "value": "[{\"from\":\"09:12\",\"index\":1,…}]" },
    { "name": "comment", "value": "" },
    { "name": "day_type", "value": "1" },
    { "name": "duty", "value": "0" },
    { "name": "only_save", "value": "0" }
  ],
  "tasks": [
    {
      "from": "09:12",
      "index": 1,
      "result": "",
      "task": "PROJ-412 Fix session timeout on the settings page (100%)",
      "time": "",
      "to": "13:30",
      "total_ts": "04:18"
    }
  ],
  "plugins": ["outlook"]
}
```

Without SiServer, a day that goes to plugins only is previewed as that: the
text says nothing is posted to SiServer, and the JSON has `"url": null`,
`"fields": []` and `"tasks": null` beside the `plugins`. With neither, the
preview fails as `--send` would.

A day that cannot be sent - no workday, or no tasks - is exit code 1 with
`--json`, rather than the message the table form prints: there is no payload
to print, and an empty answer with exit 0 would read as a day that sends
nothing.

## JSON output

`kasl report --json` prints the day the table shows, as one document - the
same reading, not a second one, so the two cannot disagree. The conventions
every kasl document keeps are in
[kasl in scripts and CI](/guides/scripting/#json-for-programs).

```json
{
  "date": "2026-08-08",
  "workday": { "start": "2026-08-08T09:12:00", "end": null },
  "intervals": [
    { "start": "2026-08-08T09:12:00", "end": "2026-08-08T13:30:00", "seconds": 15480 },
    { "start": "2026-08-08T14:18:00", "end": "2026-08-08T16:02:00", "seconds": 6240 },
    { "start": "2026-08-08T16:29:00", "end": "2026-08-08T18:04:00", "seconds": 5700 }
  ],
  "work_seconds": 27420,
  "productivity": 96.1,
  "below_threshold": false,
  "dropped": { "count": 0, "seconds": 0 },
  "tasks": [
    {
      "id": 3,
      "chain_id": 3,
      "created_at": "2026-08-08T10:05:41",
      "name": "PROJ-419 Draft migration for protected pauses",
      "comment": "backfill pending",
      "completeness": 60,
      "tags": ["backend", "db"],
      "issue_key": null
    }
  ],
  "inbox": { "total": 3, "fresh": 1, "taken": 1 }
}
```

| Field | Meaning |
| --- | --- |
| `workday` | When the day began and ended; `end` is `null` while it is open, and the whole field is `null` for a day nothing was recorded on. |
| `intervals` | The stretches of work the table lists - short ones already left out. |
| `work_seconds` | Their sum: the table's TOTAL. |
| `productivity` | The table's PRODUCTIVITY; `null` without a workday. |
| `below_threshold` | Whether the day is below the configured minimum yet - the gate `--send` enforces. |
| `dropped` | The short intervals left out, and how long they were together. |
| `tasks` | The day's tasks, in the shape [`task list --json`](/reference/task/#json-output) prints. |
| `inbox` | What the closing line counts: issues waiting, of them new, of them taken. |

## Related commands

- [`watch`](/reference/watch/) - Monitor activity for report data
- [`task`](/reference/task/) - Manage tasks included in reports
- [`pauses`](/reference/pauses/) - Record absences reflected in reports
- [`sum`](/reference/sum/) - Generate monthly summaries
- [`inbox`](/reference/inbox/) - The issues counted at the end of the report
- [`server`](/reference/server/) - The team server, and what it stores about you
