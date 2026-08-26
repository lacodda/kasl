---
title: "A day with kasl"
sidebar:
  order: 1
---

The point of kasl is that most of the day needs nothing from you. The daemon
records the workday while you work; you decide what to call the work and when to
file it. This is that day, start to finish.

## Once: start the daemon

```bash
kasl watch
```

It goes to the background and returns the PID. Everything below assumes it is
running - without it there are no intervals, no pauses and no workday.

Better still, have it start itself:

```bash
kasl autostart enable
```

On Windows this registers a Task Scheduler entry, falling back to the Registry
run key without admin rights; on macOS it writes a LaunchAgent, on Linux a
systemd user unit. After that the day starts recording when you do.

## Morning: what am I working on

Tasks do not have to be typed from memory. If GitLab or Jira is configured,
kasl offers today's commits and resolved issues as candidates:

```bash
kasl task find
```

Pick from the list. Anything already logged is filtered out, so running it again
later in the day offers only what is new.

For work that leaves no commit, add it directly:

```bash
kasl task add --name "Design review for the export screen"
```

If the same kind of task recurs, keep it as a template and stop retyping it:

```bash
kasl template add --name standup --task-name "Daily standup" --completeness 100
kasl task add --template standup
```

Assigned Jira issues arrive on their own. The daemon polls them into an inbox
and raises a desktop notification when something new lands on you:

```bash
kasl inbox          # what is waiting
kasl inbox take PROJ-412   # turn one into a task
```

## During the day: the parts the monitor cannot see

The monitor watches the keyboard and the mouse. An hour in a meeting room, a
call taken away from the desk, a lunch that ran long - none of it leaves a
trace, and all of it counts against the day if left unrecorded.

Put it on the record with its real time:

```bash
kasl pauses add --start 13:00 --minutes 45 --reason lunch
```

Add `--keep` when the absence is deliberately short and must survive the
minimum-duration filter.

Update progress as work moves:

```bash
kasl task list          # ids for today
kasl task edit 3        # name, comment, completeness
```

## End of day: look, then file

Look at the day before sending it. The intervals, the breaks and the
productivity figure were all recorded as they happened:

```bash
kasl report
```

If the productivity figure looks wrong, it usually means an absence is missing
rather than that the day went badly - see [Pauses](/reference/pauses/). Record
it and look again.

When the day reads true, file it:

```bash
kasl report --send
```

Forgot yesterday? `kasl report --last --send` files the previous day instead.

Closing the day by hand is only needed when the daemon did not do it for you:

```bash
kasl end
```

## End of month

```bash
kasl sum            # the month so far, day by day
kasl sum --send     # file the monthly summary
```

For a time sheet that wants hours rather than intervals, export the hourly
breakdown:

```bash
kasl export report --format excel --hourly
```

## What this costs you per day

Two commands worth of attention: `kasl task find` in the morning and
`kasl report --send` at the end, plus a `pauses add` whenever you step away from
the keyboard for something that counts. Everything else the daemon did while you
were working.

## Related pages

- [`watch`](/reference/watch/) - the daemon and its settings
- [`report`](/reference/report/) - the daily report and submission
- [`pauses`](/reference/pauses/) - recording absences the monitor missed
- [`inbox`](/reference/inbox/) - the Jira inbox in detail
- [Configuration](/concepts/configuration/) - thresholds behind all of this
