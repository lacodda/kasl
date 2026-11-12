---
title: "status"
description: One line for a shell prompt or a status bar - today's work so far, a pause the watcher sees or a watcher that is not running, and the inbox.
---

The `status` command prints where today stands in one line, made for a shell
prompt or a status bar.

## Usage

```bash
kasl status [--json]
```

- `--json`: Print the same as one JSON document, field by field - see [JSON output](#json-output).

```text
$ kasl status
6h 40m | paused 12m | inbox 9 (3 new)
```

## The line

The line is made of segments, each there only when it has something to say:

- **the day** - today's work so far: the TOTAL [`kasl report`](/reference/report/)
  shows, counted the same way. Absent until the watcher has opened today's
  workday.
- **the watcher** - `paused 12m` while the [watcher](/reference/watch/) sees a
  pause, measured from when the keyboard went quiet. `watch off` when no
  watcher is running: then nothing is being recorded at all, which is the one
  thing a time tracker must not let you miss. A day closed with
  [`kasl end`](/reference/end/) shows no pause, however quiet the keyboard.
- **the inbox** - `inbox 9` for the issues waiting in the
  [inbox](/reference/inbox/), and `(3 new)` for the ones that arrived recently.
  The same counts the report's inbox line gives.

Nothing to say - the watcher runs, the day has not started, the inbox is
empty - prints nothing, not even an empty line, so a prompt segment built on
it simply disappears. Only at a terminal does `status` say why the line is
empty instead.

The line is plain ASCII - segments are joined by ` | ` - so a prompt shows it as it is in any console code page; PowerShell decodes what it captures from a program in the console's code page, and a `·` would come back garbled there.

The line is quick and quiet: it reads the local database and the watcher's PID
file, never the network, and never stops to check for a newer release - a
prompt runs it every time it draws. Wiring it into starship, tmux, PowerShell,
bash and zsh: [The day in your prompt](/guides/prompt-line/).

### Which pause is "now"

The watcher writes a pause down when it opens it and closes the row when you
come back, so an open row in the database is a pause in progress - unless the
watcher that opened it was killed before it could close it. Such a row stays
open for good. `status` counts an open pause only when the watcher running
now opened it: one that began after that watcher started. A pause an earlier
watcher left behind never reads as a break happening now.

### While a pause runs

The day's figure is the report's, and the report counts a pause in progress as
work until it ends: it becomes a pause once it is over, and then only if it
lasted `min_pause_duration`. So during a long break the day's figure keeps
growing next to `paused 25m`, and drops back when you return. That is the
report's arithmetic, kept here on purpose - the prompt never shows a number
`kasl report` would not.

## JSON output

`kasl status --json` prints today as one document (conventions:
[kasl in scripts and CI](/guides/scripting/#json-for-programs)):

```json
{
  "date": "2026-11-12",
  "workday": { "start": "2026-11-12T09:12:04", "end": null },
  "work_seconds": 24000,
  "watcher": true,
  "state": "paused",
  "paused_since": "2026-11-12T15:40:11",
  "inbox": { "total": 9, "fresh": 3, "taken": 2 },
  "line": "6h 40m | paused 12m | inbox 9 (3 new)"
}
```

- `workday` is `null` until the watcher opens today's workday.
- `work_seconds` is the report's `work_seconds` for today.
- `watcher` says whether a watcher runs.
- `state` is `working`, `paused` or `idle` - the words the
  [pulse](/reference/server/#kasl-server-pulse) uses - and `null` when no
  watcher runs, because then nobody is watching to say.
- `paused_since` is when the pause began, while `state` is `paused`, and
  `null` otherwise.
- `inbox` holds the same counts as the report's `inbox`.
- `line` is the line `kasl status` prints, `""` when there is nothing to say.

A bar that draws its own pieces - waybar, polybar, a Rainmeter skin - reads
the fields; one that only shows text reads `line`.

## Examples

```bash
# The line, for a prompt
kasl status

# Hours worked today, for a script
kasl status --json | jq '.work_seconds / 3600'

# Warn when nothing is recording
kasl status --json | jq -e '.watcher' > /dev/null || echo "kasl is not watching"
```

## Related commands

- [`report`](/reference/report/) - The whole day the first segment sums up
- [`watch`](/reference/watch/) - The watcher whose pause and absence the line reports
- [`inbox`](/reference/inbox/) - The issues the last segment counts
- [`standup`](/reference/standup/) - The day or the week as markdown
