---
title: "demo"
description: Eight invented weeks to try kasl on - workdays, pauses, tasks and an inbox, kept apart from your own records, with every command running against them.
---

The `demo` command builds eight invented weeks of work and runs any other
command on them. Nothing has to be configured, nothing has to be recorded
first, and your own records are neither read nor changed.

## Usage

```bash
kasl demo [--reset]
kasl demo <command> [arguments]
```

- `kasl demo`: Build the demo when there is none, and say what to try on it.
- `kasl demo <command>`: Run that command on the demo: `kasl demo report`,
  `kasl demo task list --all`, `kasl demo inbox --json`. The word goes right
  after `kasl` (or after `--plain`), before the command.
- `--reset`: Build the demo anew, dropping whatever was added to it.

```console
$ kasl demo
The demo is built: 36 working days from October 5 to November 25, 48 tasks and an inbox of 14 issues - all invented, and kept apart from your own records.
Today in it: 4h 7m worked so far.

Put `demo` before any command to run it there:

  kasl demo report          today: the hours between pauses, and the tasks
  kasl demo report --last   yesterday, a finished day
  kasl demo sum             the month, day by day
  kasl demo standup --week  the week as markdown, for a standup
  kasl demo task list       today's tasks
  kasl demo inbox           the issues waiting, as a tracker would fill it
  kasl demo status          one line for a shell prompt

Anything you add there stays there, until the next day or `kasl demo --reset` builds the demo anew.
Your own day starts with `kasl watch`.
```

There is no need to run `kasl demo` first: the first command given after the
word builds the demo too.

## What is in it

One invented person's work on one invented product:

- **Workdays** - every weekday of the last eight weeks, this week included,
  ending today. Today's day is open, as a day being worked is. Among them a
  couple of days of leave, two long days, one with an appointment recorded by
  hand, and one interrupted often enough to fall under the productivity
  minimum - so `sum` and `report` show what they say about each.
- **Pauses** - lunch every day, a longer break most afternoons, and the short
  absences productivity is measured by.
- **Tasks** - two pieces of work in hand at a time, each carried over its days
  from a first quarter to done, with tags and comments; a code review twice a
  week, planning and a retrospective every other week.
- **An inbox** - issues of a project called `ORBIT`: three that arrived today,
  one pinned, one whose status changed, one asleep, one dismissed, the ones in
  hand marked taken, and the ones finished lately resolved.
- **Tags and templates** to go with them.

```console
$ kasl demo report --last
Report for November 24, 2026

+--------------+-------+-------+----------+
| ID           | START | END   | DURATION |
+--------------+-------+-------+----------+
| 1            | 09:29 | 12:36 | 03:07    |
| 2            | 13:18 | 16:04 | 02:46    |
| 3            | 16:27 | 17:54 | 01:27    |
|              |       |       |          |
| TOTAL        |       |       | 07:20    |
| PRODUCTIVITY |       |       | 96.2%    |
+--------------+-------+-------+----------+

Tasks:

+---+----+---------+-------------------------------------+-------------------+------+--------------+
| # | ID | TASK ID | NAME                                | COMMENT           | DONE | TAGS         |
+---+----+---------+-------------------------------------+-------------------+------+--------------+
| 1 | 90 | 90      | Code review                         |                   | 100% | review       |
| 2 | 91 | 85      | ORBIT-449 Orbit refresh fails with… | keep the last go… | 100% | backend, bug |
| 3 | 92 | 86      | ORBIT-452 Group stations by region  |                   | 100% | frontend     |
+---+----+---------+-------------------------------------+-------------------+------+--------------+
```

The weeks are generated, not shipped: a day's hours depend on its date alone,
so two demos built on the same day are the same demo, and a day looks the same
tomorrow as it did today.

## A place of its own

The demo lives in a directory called `demo` inside kasl's
[data directory](/concepts/configuration/), with its own database and its own
config file. A command run after the word works there and nowhere else:

- `kasl demo task add --name "Try this"` adds a task to the demo. `kasl task
  list` never shows it, and `kasl demo task list` never shows a task of yours.
- `kasl demo config set ...` changes the demo's settings, not yours.
- Secrets asked for inside the demo are kept in the OS keyring under a name of
  their own, apart from the ones your installation uses.
- Plugins installed on the machine are not told about the demo's days and do
  not fill its inbox.

What you add stays until the demo is built anew: by `kasl demo --reset`, or by
itself the first time it is used on another day - a demo built yesterday has
no today in it - or after kasl is updated. kasl says so, beside the output,
when that happens.

## What the demo refuses

Commands that act on the machine or reach a server, rather than on a data
directory, do not run in the demo and say which command to run instead:

- `kasl demo watch` and `kasl demo autostart` - the watcher records real
  activity, and the demo's days are invented.
- `kasl demo server` - invented days go to no team server.
- `kasl demo self-update` - there is nothing of the demo's to update.

`kasl demo status` therefore always says `watch off`: no watcher runs there.

## Examples

```bash
# Build the demo and read the introduction
kasl demo

# A finished day, the month, the week
kasl demo report --last
kasl demo sum
kasl demo standup --week

# The inbox, in every state it can show
kasl demo inbox --all --snoozed --dismissed

# Try a change, then start over
kasl demo task add --name "Try this" --completeness 50
kasl demo report
kasl demo --reset

# The demo's documents, for a script or a status bar under development
kasl demo status --json
```

## Related commands

- [`report`](/reference/report/) - The day the demo fills in
- [`sum`](/reference/sum/) - The month, day by day
- [`inbox`](/reference/inbox/) - The issues the demo invents
- [`watch`](/reference/watch/) - Where your own records start
