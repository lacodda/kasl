---
title: "pauses"
---

The `pauses` command shows the absences recorded during a workday and lets you record the ones the activity monitor missed.

Most pauses arrive on their own: the `watch` daemon notices that input has stopped and writes a pause when activity resumes. But the monitor only sees the keyboard and mouse, so an absence spent away from the machine - a meeting in another room, a walk with the laptop shut - leaves no trace. `pauses add` is how you put it on the record.

## Usage

```bash
kasl pauses [SUBCOMMAND]
```

| Subcommand | Purpose |
| --- | --- |
| `list` | Show pauses for a date |
| `add` | Record an absence the monitor did not detect |
| `remove` | Delete a pause record |

Running `kasl pauses` with no subcommand lists today's pauses.

## `kasl pauses list`

```bash
kasl pauses list [OPTIONS]
```

- `-d, --date <DATE>`: Date to fetch pauses for (default: `today`). Accepts `today` or `YYYY-MM-DD`.
- `-m, --min-duration <MINUTES>`: Only show pauses at least this long. Overrides the configured `min_pause_duration`.

Protected pauses (see below) are always listed, whatever the threshold.

## `kasl pauses add`

```bash
kasl pauses add --start <HH:MM> --minutes <N> [OPTIONS]
```

- `-s, --start <HH:MM>`: When the absence began. Required.
- `-m, --minutes <N>`: How long it lasted, in minutes. Required.
- `-d, --date <DATE>`: Date the absence belongs to (default: `today`).
- `--keep`: Protect the pause from filtering and merging.
- `-r, --reason <TEXT>`: Optional note describing the absence.

You state the time; nothing is inferred. If the entry would overlap a pause already on record, it is rejected rather than silently merged - a day should not hold two contradictory accounts of the same minutes.

### Protected pauses (`--keep`)

Ordinary pauses pass through two filters before they reach a report: pauses shorter than `min_pause_duration` are dropped, and pauses separated by a negligible burst of activity are merged into one. Both filters exist to clean up noise from the monitor.

A pause you entered by hand is not noise. `--keep` marks it protected, which exempts it from both: a deliberately short entry survives the threshold, and an entry adjacent to a detected pause keeps its own bounds instead of being absorbed.

Use it when the absence is real but short enough that the threshold would discard it:

```bash
kasl pauses add --start 16:20 --minutes 10 --keep --reason "stand-up in the other room"
```

## `kasl pauses remove`

```bash
kasl pauses remove <ID> [-y]
```

- `<ID>`: Id of the pause to remove, as shown by `kasl pauses list`.
- `-y, --yes`: Remove without asking for confirmation.

Without `-y` the command asks before deleting. Outside an interactive terminal it refuses instead of prompting, so scripts fail loudly rather than hang.

## Examples

```bash
# Show today's pauses
kasl pauses

# Show pauses for a specific date
kasl pauses list --date 2026-08-07

# Show only significant breaks
kasl pauses list --min-duration 30

# Record an hour-long lunch the monitor missed
kasl pauses add --start 13:00 --minutes 60 --reason "lunch"

# Record a short absence that must survive the duration filter
kasl pauses add --start 16:20 --minutes 10 --keep

# Record an absence on an earlier day
kasl pauses add --date 2026-08-07 --start 11:30 --minutes 45

# Remove a pause entered by mistake
kasl pauses remove 42
```

## Sample Output

```
August 8, 2026

+--------------+-------+-------+----------+
| ID           | START | END   | DURATION |
+--------------+-------+-------+----------+
| 1            | 10:30 | 10:45 | 00:15    |
| 2            | 12:00 | 13:00 | 01:00    |
| 3            | 15:15 | 15:30 | 00:15    |
|              |       |       |          |
| TOTAL        |       |       | 01:30    |
+--------------+-------+-------+----------+
```

## How pauses affect productivity

Productivity is the share of available time you were actually at the machine:

```text
Available Work Time = Workday Length - Long Pauses
Net Work Time       = Available Work Time - Short Pauses
Productivity        = Net Work Time / Available Work Time * 100
```

Long pauses - detected absences at or above `min_pause_duration`, plus every manual pause you record - are time you were away, so they leave the denominator entirely. Short pauses are brief interruptions while you were present, so they lower the numerator only.

Recording a genuine absence therefore raises productivity: the time no longer counts against you. That is the honest use of `pauses add`, and the only one it supports - it records absences, it does not manufacture them.

> **Note:** earlier versions had a separate `kasl breaks` command that invented break times to lift the productivity figure above the reporting threshold. It has been removed. Existing break records were migrated into the pause list as protected pauses, so historical reports keep their numbers.

## Related Commands

- **[`report`](/reference/report/)** - View the complete workday summary including pauses
- **[`watch`](/reference/watch/)** - Monitor activity and detect pauses automatically
- **[`export`](/reference/export/)** - Export pause data for external analysis
