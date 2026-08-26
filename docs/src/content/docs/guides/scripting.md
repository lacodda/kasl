---
title: "kasl in scripts and CI"
sidebar:
  order: 3
---

kasl is a CLI you type into, but most of its commands also run unattended:
from cron, from a CI job, from a script another tool shells out to. Nobody is
there to answer a prompt in those places, so kasl draws a hard line - any
command that would have asked a question either has everything it needs on
the command line, or it fails immediately with an error naming the flag that
was missing. It never sits there waiting.

## The principle

kasl decides whether a terminal is present by checking if stdin is a TTY. A
prompt that would otherwise block forever - under the daemon, in CI, behind a
pipe - is replaced by an immediate, explanatory error instead:

```bash
kasl task add < /dev/null
```

```
Error: task name is required; pass --name outside an interactive terminal
```

That message is the contract: every command that refuses this way tells you
exactly which flag makes it work without a terminal. This guide lists them.

## Commands that already work without a terminal

Most of kasl needs nothing extra to run unattended, because they don't prompt
in the first place - all of these ran clean with stdin closed:

```bash
kasl report < /dev/null
kasl sum < /dev/null
kasl task list < /dev/null
kasl pauses list < /dev/null
kasl inbox list < /dev/null
kasl export report --format csv < /dev/null
kasl end < /dev/null
kasl completions bash < /dev/null
```

`report` and `sum` print whatever the database has (or a "no workday record"
message) and exit 0; `export` exits 1 if there is nothing to export, same as
it would interactively.

## Commands that need flags instead of answers

These commands prompt when run at a terminal. Outside one, supply the same
information as flags and they skip the prompts entirely.

### `task add` and `template add`

```bash
kasl task add --name "Ship the scripting guide"
```

`--name` is required outside a terminal for both commands - without it:

```
Error: task name is required; pass --name outside an interactive terminal
```

`--comment`, `--completeness` and (for `task add`) `--tags` are optional -
anything not supplied is left blank or defaulted, since there is no one to
ask. For `template add`, `--task-name` defaults to the template's own name.

### `task add --template`

```bash
kasl template add --name standup --task-name "Daily standup" --comment "Sync with team" --completeness 100
kasl task add --template standup
```

With no overrides, the task is created exactly as the template stores it. Any
of `--name`, `--comment`, `--completeness` or `--tags` given on the command
line overrides the template's value for that field only:

```bash
kasl task add --template standup --comment "Overridden comment" --completeness 50 --tags cron,automated
```

### `task remove` / `template remove`

```bash
kasl task remove 3 --yes
kasl template remove standup --yes
```

Without `--yes`, outside a terminal, both refuse (`Error: refusing to remove
tasks without --yes outside an interactive terminal`) - deliberately: a
destructive command doesn't get to assume "yes" just because nobody could say
"no". Interactively, `--yes` skips the same confirmation prompt.

### `pauses add` and `inbox` subcommands

```bash
kasl pauses add --start 13:00 --minutes 45
kasl inbox take PROJ-412
kasl inbox dismiss PROJ-412
kasl inbox pin PROJ-412
kasl inbox open PROJ-412
```

`pauses add` takes an explicit start time and duration - no prompts to begin
with. Every `inbox` subcommand that acts on one issue takes its key as an
argument; none of them fall back to an interactive picker.

## Commands that require a terminal, on purpose

A few commands are inherently a conversation and refuse outright outside one,
rather than pretending a flag could stand in for a human decision. Each fails
with a message naming itself, e.g. `` `kasl task find` is interactive and
needs a terminal ``:

- **`kasl setup`** - the configuration wizard walks through several
  interdependent choices; no single flag replaces it.
- **`kasl task find`** - offers commits and issues as candidates and asks you
  to pick; nothing to script against.
- **`kasl task edit`** without an id - picks a task from a list first.
- **`kasl task add --from-template`** - picks a template from a list. Use
  `kasl task add --template NAME` instead, which names the template directly
  and works without a terminal.

## Environment variables

- **`KASL_DEBUG=1`** or **`RUST_LOG=kasl=debug`** - diagnostic logging to
  stderr. kasl keeps no log file; see
  [Troubleshooting](/guides/troubleshooting/) for detail and
  `RUST_LOG=kasl=trace` for more of it.
- **`KASL_VERSION`**, **`KASL_INSTALL_DIR`**, **`KASL_NO_ALIAS`** - read by the
  install scripts, not by kasl itself. See
  [Getting Started](/getting-started/#installer-options).

There is no environment-variable override for configuration values - settings
live only in `config.json` and the OS keyring (see
[Configuration](/concepts/configuration/)).

## Exit codes and a cron example

Success is exit code 0; any failure - a refused prompt, a missing workday, a
rejected report - is exit code 1. A nightly job, run weekdays at 20:00
(`0 20 * * 1-5 kasl-nightly-report.sh`), that files the day's report and only
makes noise on failure relies on exactly this:

```bash
#!/usr/bin/env bash
set -euo pipefail

if ! kasl report --send > /tmp/kasl-report.log 2>&1; then
    echo "kasl report --send failed:" >&2
    cat /tmp/kasl-report.log >&2
    exit 1
fi
```

The same non-zero exit covers a CI step - no extra error handling is needed
for the runner to mark it failed, even when the failure is the productivity
gate in [Configuration](/concepts/configuration/) rejecting the report:

```yaml
- name: Record and send the day's report
  run: |
    kasl task add --name "CI: ${{ github.workflow }}" --completeness 100
    kasl report --send
```

## The one command that starts a background process

`kasl watch` is the exception to "runs and exits": it forks a daemon and
returns immediately - the right thing at login, not what a script expects
from a command that should finish before returning. `kasl watch --stop` stops
it; `kasl watch --foreground` stays in the terminal, for debugging rather than
scripts. Calling `kasl watch` again is not a no-op: it stops the
daemon already running, waits for it to clean up, and starts a fresh one. That
keeps a single watcher on the machine, but it means a scheduled job calling it
restarts monitoring every time. Start it once - at login, or through
[`autostart`](/reference/autostart/) - rather than from a recurring job.

Scripts that only record something (a task, a pause, a report) don't need the
daemon at all.

## Related pages

- [A day with kasl](/guides/a-day-with-kasl/) - the same commands from the
  interactive side
- [Troubleshooting](/guides/troubleshooting/) - debug logging and where kasl
  keeps its files
- [`task`](/reference/task/) - full flag reference
- [`template`](/reference/template/) - full flag reference
- [`report`](/reference/report/) - daily report and submission
- [`watch`](/reference/watch/) - the daemon in detail
- [Getting Started](/getting-started/) - installation and the installer's own
  environment variables
- [Configuration](/concepts/configuration/) - where settings actually live
