---
title: "Pauses and the productivity figure"
sidebar:
  order: 5
---

The number at the bottom of `kasl report` is the one people ask about first,
usually because it looks lower than the day felt. It is worth understanding what
it measures, because the fix for a wrong-looking figure is almost always to
record something that happened, not to adjust the number.

## What the monitor sees

The daemon watches keyboard and mouse activity and nothing else. It knows you
were at the machine; it does not know why you were not.

- **Sustained activity** starts the workday - a stray mouse nudge does not
  (`activity_threshold`, 30 seconds by default).
- **Silence** past `pause_threshold` (60 seconds) opens a pause.
- **Activity again** closes it.

Two consecutive pauses separated by no more than `pause_merge_gap` (30 seconds)
are treated as one, so a single keypress in the middle of a break does not split
it into halves that then fall below the recording threshold.

## Short and long pauses are not the same thing

This split drives the whole calculation. The dividing line is
`min_pause_duration`, 20 minutes by default:

- A **short pause** is an interruption. You were at work; you stopped typing for
  a while. It counts against the day.
- A **long pause** is an absence. You were not there. It is taken out of the
  day, rather than counted against it.

That gives:

```text
Available Work Time = Total Time - Long Pauses
Net Work Time       = Available Work Time - Short Pauses
Productivity        = Net Work Time / Available Work Time * 100
```

A two-hour lunch does not sink the figure - it shortens the day. Forty minutes
of staring out of the window does lower it, which is the point.

## Why the figure is usually wrong in one specific way

The monitor cannot see a meeting room. An hour spent in a meeting, a call taken
away from the desk, a conversation at someone else's monitor - all of it looks
exactly like an hour of silence at your keyboard. It becomes a pause, and
depending on the length, either counts against you or is simply missing from the
record.

The fix is to put the absence on the record with its real time:

```bash
kasl pauses add --start 14:00 --minutes 60 --reason "planning meeting"
kasl pauses list
```

Now the hour leaves the available time instead of dragging the ratio down, and
the day reads as what it was.

For a deliberately short absence that must survive the minimum-duration filter,
add `--keep`:

```bash
kasl pauses add --start 11:20 --minutes 10 --reason "standup" --keep
```

A protected pause is never dropped by the filter and never merged into a
neighbour - it stays exactly as recorded.

## The threshold that blocks submission

`report --send` refuses to submit a day whose productivity is below
`min_productivity_threshold` (75% by default), and says which number it saw
against which limit. The monthly `sum --send` has no such check.

A softer warning appears in the report itself, and that one waits until
`min_workday_fraction_before_suggest` of the expected workday has elapsed (half
of it by default): early in the morning the ratio swings wildly on a single
pause, so warning then would be noise. The submission check has no such delay -
it applies whenever you try to send.

This is a safeguard, not an obstacle: a day below the threshold usually means an
absence is missing from the record. Recording the absence is the supported way
to raise the number - and the honest one, since the alternative is filing a day
that did not happen.

## Removing a pause recorded by mistake

```bash
kasl pauses list           # find the id
kasl pauses remove 4       # asks for confirmation
kasl pauses remove 4 --yes # skips it, for scripts
```

## Tuning the thresholds

Every number above lives in the configuration and can be changed:

```bash
kasl setup   # the Monitor and Productivity modules
```

`min_pause_duration` is the one worth thinking about. Lower it and more of the
day's gaps are treated as absences, shortening the day; raise it and they count
against the ratio instead. The defaults suit a day at a desk; a job with many
short meetings may want a lower one.

See [Configuration](/concepts/configuration/) for every field and its default.

## Related pages

- [`pauses`](/reference/pauses/) - the command in full
- [`report`](/reference/report/) - where the figure appears
- [Configuration](/concepts/configuration/) - monitor and productivity settings
- [A day with kasl](/guides/a-day-with-kasl/) - the day these numbers describe
