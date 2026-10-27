---
title: "end"
---

The `end` command writes the end timestamp for today's workday, closing the day
by hand, and tells the [plugins](/reference/plugin/) that hear `workday_end`.

## Usage

```bash
kasl end
```

The command takes no arguments.

## When You Need It

The watcher opens a day but does not close it: an unclosed day is read up to
the last activity it saw (see [A day that was never
closed](/reference/report/#a-day-that-was-never-closed)), and stays unclosed in
the database. `end` writes the end down.

- **You are leaving now and want the day closed now.** This stamps the end at
  the moment you run it, rather than at whatever activity comes last.
- **The last stretch was not at the keyboard.** A meeting or a call that ended
  the working day leaves no activity for the monitor to see.
- **Something should hear that the day is over.** A plugin that mails the day
  or files it somewhere hears `workday_end` when an end is written.

## What It Does

Records the current time as the end of today's workday. The day's intervals and
pauses are untouched - only the closing timestamp is written.

Running it again overwrites the timestamp with the new current time, so a day
closed too early can be closed again later.

Then every plugin that is set up and hears `workday_end` is handed the day, as
`kasl report --json` prints it, its new end included. Each one's answer is
printed after the command's own line; a plugin that fails is a warning, and the
day stays closed. Ending the day twice tells them twice, with the later end.

`kasl report --send` closes a day by the same rule, and for a past day the rule
is different: a past day ends at its last observed activity, and one already
closed keeps its end.

The command closes a day that exists; it never opens one. If nothing was
recorded for today - the watcher never ran - there is no day to close, and
`end` says so and fails rather than reporting a stamp it did not write.

## Sample Output

```
ℹ️ Workday ended for today.
```

With a plugin that hears it:

```
ℹ️ Workday ended for today.
ℹ️ outlook: Mailed the day to me@example.com
```

With no workday for today:

```
❌ No workday was started on 2026-01-15, so there is nothing to end. `kasl watch` opens the day, and `kasl report` shows what is recorded.
```

## Examples

```bash
# Close the day and look at what it came to
kasl end
kasl report

# Close the day and file the report
kasl end
kasl report --send
```

## Related commands

- **[`watch`](/reference/watch/)** - The daemon that normally opens and closes the day
- **[`report`](/reference/report/)** - The day's intervals, tasks and productivity
- **[`pauses`](/reference/pauses/)** - Record an absence the monitor missed
- **[`plugin`](/reference/plugin/)** - The plugins that hear `workday_end`
