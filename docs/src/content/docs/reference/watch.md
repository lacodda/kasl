---
title: "watch"
---

The `watch` command is kasl's activity monitor. It watches keyboard and mouse input to detect when a workday starts, records pauses when input stops, and closes the day when activity ends for good.

It also does two things for the [Jira inbox](/reference/inbox/): it polls Jira on its own cadence, and it is what answers the **Take** / **Snooze** / **Dismiss** buttons on an inbox toast. Both run in daemon and `--foreground` modes alike. With the watcher stopped, a toast button still works - the press is carried out on the spot instead - but nothing polls Jira, so no new toast appears.

## Usage

```bash
kasl watch [OPTIONS]
```

- `--foreground`: Run in the current terminal instead of as a background daemon, with real-time feedback about detected activity, pause events, and workday state changes. Useful for testing configuration changes.
- `-s, --stop`: Stop the running background watcher, closing its database connections cleanly.

Running `kasl watch` with no options starts the background daemon - the normal way to use it day to day.

## One watcher at a time

There is one watcher per user, however it is started. `kasl watch` stops the watcher it knows about and starts a new one; a watcher it does not know about - started by another copy of kasl, or at the same moment by two autostart entries - is left running, and `watch` says so instead of starting a second. `--foreground` refuses while a watcher runs: stop it with `kasl watch --stop` first. Two watchers would poll Jira twice and show every toast twice.

`--stop` stops only a kasl process: when the recorded process has ended and its number now belongs to another program, the stale record is cleared and nothing is killed.

## Examples

```bash
# Start monitoring in the background
kasl watch

# Run in the foreground to see activity detection live
kasl watch --foreground

# Stop the background daemon
kasl watch --stop
```

## Configuration

Detection thresholds - how long a pause has to last to count, how much activity starts a workday, the polling interval - are read from the config file rather than passed as flags. See [Configuration](/concepts/configuration/).

## Debug logging

`watch` uses `tracing` for its internal logging, written to stderr. Logging is off by default; set `RUST_LOG` (e.g. `RUST_LOG=kasl=debug`) or `KASL_DEBUG=1` to enable it:

```bash
RUST_LOG=kasl=debug kasl watch --foreground
```

## Related commands

- [`task`](/reference/task/) - Manage tasks while monitoring runs
- [`report`](/reference/report/) - View the day built from monitored activity
- [`pauses`](/reference/pauses/) - Record an absence the monitor missed
- [`setup`](/reference/setup/) - Configure monitoring thresholds
