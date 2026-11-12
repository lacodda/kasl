# ADR 0012: The status line reads the running watcher's pause, and the report's day

- Status: accepted
- Date: 2026-11-12

## Context

`kasl status` is one line for a shell prompt or a status bar: today's work, whether the watcher sees a pause, the inbox. A prompt runs it every time it draws, so it has to be quick, local and quiet, and what it says has to be true now.

"Is the watcher in a pause" lives in the watcher's memory - the flag the pulse reads - and nowhere a second process can read it. The database has a shadow of it: the watcher writes a pause row when the keyboard goes quiet (`end IS NULL`) and closes it when input returns. But a watcher killed mid-pause never closes its row, and nothing closes it later: the next watcher starts active, and closes only the pauses it opens. So "the newest open pause" can be a break that ended hours ago, under a watcher that is gone.

The day's figure has a second question: the report counts a pause only once it is over (and only if it lasted `min_pause_duration`), so a pause in progress is work until it ends.

## Decision

**A pause is in progress when the running watcher opened it.** The watcher writes its PID file as it takes the lock, before its monitor sees an event; the file's modification time is when that watcher started (`daemon::running_since`, which also checks the PID names a live kasl process). An open row stamped at or after that moment - a second of slack, since stamps are whole seconds - is the running watcher's own; one stamped before it was left by a dead watcher and is ignored. No watcher, no pause: the line says `watch off`, and `--json` says `state: null` rather than guessing.

**The state is the pulse's.** `pulse::presence` turns "in a pause" and today's workday into `working`, `paused` or `idle`, so a day closed with `kasl end` is idle however quiet the keyboard - exactly what a team server would be told.

**The day's figure is the report's.** It comes from `DayWork::of`, the function `kasl report`, `sum`, the week and the MCP tools read days through. During a long break it keeps growing next to `paused 25m` and drops back when the pause closes. Counting the open pause in the status line alone would make the prompt and the report disagree about the same day; the open pause belongs in `DayWork`, for every reader at once, or nowhere.

**No update check.** `status` is exempt from the once-a-day release check like the toast courier is: a prompt would otherwise wait on GitHub once a day.

## Consequences

The line never reports a break that a dead watcher left behind, and never claims a state nobody is watching. It reads one PID file, one process handle and the local database - no network.

Counting a pause in progress in `DayWork` itself - so the report, the status line and the pulse all stop counting a break as work while it runs - is left as its own change: it touches every reader of a day, and it needs the same "which open pause is live" rule this ADR introduces, which is now there to use.
