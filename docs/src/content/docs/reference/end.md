---
title: "end"
---

The `end` command writes the end timestamp for today's workday, closing the day
by hand.

## Usage

```bash
kasl end
```

The command takes no arguments.

## When You Need It

The watcher normally closes the day on its own: it notices when activity stops
and finalises the workday. `end` exists for the times it cannot.

- **The watcher was not running.** A day recorded without the daemon has a start
  but no end until you say so.
- **You are leaving now and want the day closed now.** Rather than letting the
  daemon decide later, this stamps the end at the moment you run it.
- **The last stretch was not at the keyboard.** A meeting or a call that ended
  the working day leaves no activity for the monitor to see.

## What It Does

Records the current time as the end of today's workday. The day's intervals and
pauses are untouched - only the closing timestamp is written.

Running it again overwrites the timestamp with the new current time, so a day
closed too early can be closed again later.

The command closes a day that exists; it never opens one. If nothing was
recorded for today - the watcher never ran - there is no day to close, and
`end` says so and fails rather than reporting a stamp it did not write.

## Sample Output

```
ℹ️ Workday ended for today.
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
