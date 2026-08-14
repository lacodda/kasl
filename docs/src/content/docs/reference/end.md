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

:::caution[No workday, no record]
The command updates an existing workday. If no workday was ever started for
today - the watcher never ran and nothing was recorded - there is nothing to
update, and `end` still reports success without writing anything. Check with
`kasl report` if you are unsure whether the day exists.
:::

## Sample Output

```
ℹ️ Workday ended for today.
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

## Related Commands

- **[`watch`](/reference/watch/)** - The daemon that normally opens and closes the day
- **[`report`](/reference/report/)** - The day's intervals, tasks and productivity
- **[`pauses`](/reference/pauses/)** - Record an absence the monitor missed
