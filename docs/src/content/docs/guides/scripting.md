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
❌ task name is required; pass --name outside an interactive terminal
```

That message is the contract: every command that refuses this way tells you
exactly which flag makes it work without a terminal. This guide lists them,
and then how to read kasl's output from a program
([`--json`](#json-for-programs)) or a log ([`--plain`](#plain-text-for-logs-and-pipes)).

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
kasl server queue < /dev/null
kasl server flush < /dev/null
kasl server push < /dev/null
kasl server manifest < /dev/null
kasl report --send --show < /dev/null
```

`report` and `sum` print whatever the database has (or a "no workday record"
message) and exit 0; `export` exits 1 if there is nothing to export, same as
it would interactively.

`report --send --show` is the one member of the `--send` family that needs no
credential: it prints the payload and stops before the request, so it neither
opens a session nor reaches the network. `server manifest` does reach the
server, and exits 1 when this machine is not connected to one.

## Commands that need flags instead of answers

These commands prompt when run at a terminal. Outside one, supply the same
information as flags and they skip the prompts entirely.

### `task add` and `template add`

```bash
kasl task add --name "Ship the scripting guide"
```

`--name` is required outside a terminal for both commands - without it:

```
❌ task name is required; pass --name outside an interactive terminal
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

Without `--yes`, outside a terminal, both refuse (`❌ refusing to remove
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
- **`kasl server connect`** - the agent token is always typed at a prompt,
  never taken from an argument where it would land in shell history. Connect
  once by hand; everything that sends days afterwards runs unattended.

## JSON for programs

Every command that prints data takes `--json`: a status bar, a script or
another tool gets the same facts as the table, without scraping it.

```bash
kasl inbox list --json | jq -r '.issues[] | select(.pinned) | .key'
kasl report --json | jq '.work_seconds / 3600 | floor'
kasl task list --all --json | jq '[.tasks[] | select(.completeness < 100)] | length'
```

With `--json`, stdout carries exactly one JSON document and nothing else.
Everything else the run has to say - a warning about a skipped inbox rule, the
debug log - goes to stderr, so `kasl … --json | jq` never chokes on a stray
line. The update check is skipped too: a script never waits on GitHub.

The commands that take it:

| Command | Document |
| --- | --- |
| [`report`](/reference/report/#json-output) | the day: workday, intervals, productivity, tasks, inbox counts |
| [`report --send --show`](/reference/report/#what-gets-sent) | the payload `--send` would post |
| [`sum`](/reference/sum/#json-output) | the month, day by day |
| [`pauses list`](/reference/pauses/#json-output) | a day's pauses |
| [`task list`, `task show`](/reference/task/#json-output) | tasks |
| [`tag list`, `tag show`](/reference/tag/#json-output) | tags, and the tasks carrying one |
| [`template list`, `show`, `search`](/reference/template/#json-output) | templates |
| [`inbox list`, `inbox show`, `inbox rules`](/reference/inbox/#json-output) | issues, one issue and why it sits where it does, the rules |
| [`server status`, `queue`, `pulse`, `manifest`](/reference/server/#json-output) | the connection, the owed days, the pulse, what the server stores |
| [`autostart status`](/reference/autostart/#json-output) | whether kasl starts at login |

A command with no data to print does not take the flag: `kasl task add --json`
is a usage error, not a task added with the answer in prose. The same goes
for the short forms that carry list flags - `kasl inbox --json pin PROJ-412`
refuses before pinning anything; give `--json` to the subcommand that lists.

Every document keeps the same conventions, so a reader learns them once:

- The top level is an object, never a bare list - a field can be added later
  without breaking anyone reading the ones already there.
- Every key is always present. A value that does not exist is `null`; an empty
  list is `[]`.
- Moments are the machine's local time as `YYYY-MM-DDTHH:MM:SS` - whole
  seconds, no offset. Dates are `YYYY-MM-DD`.
- Durations are whole seconds, in fields whose names end in `_seconds`.
- Percentages are numbers from 0 to 100, rounded to one decimal.

`kasl inbox list --json` reads the local inbox table only, never Jira, so it
answers at once - which is how [rigger](https://github.com/lacodda/rigger)
shows the inbox beside its own tasks:

```bash
kasl inbox list --all --snoozed --json
```

## Plain text for logs and pipes

`--plain` works on every command and keeps the output a person reads, minus
the decoration a log or `grep` trips over: no colour, no emoji (a warning says
`warning:`, an error `error:`), no blank lines around headings, and tables as
tab-separated lines under a header row, with no cell cut to the terminal's
width.

```console
$ kasl task list --plain
#	ID	TASK ID	NAME	COMMENT	DONE	TAGS
1	1	1	PROJ-412 Fix session timeout on the settings page	stale cookie jar	100%	backend
2	2	2	Review PR #318: pause merging		100%	
3	3	3	PROJ-419 Draft migration for protected pauses	backfill pending	60%	backend, db
```

```bash
kasl inbox --plain | awk -F'\t' 'NR > 2 && $4 == "High" { print $5 }'
```

The shape never changes on its own: kasl prints the same table to a terminal
and to a pipe, and only `--plain` changes it. For anything you parse, prefer
`--json` - plain text is for reading and grepping.

Warnings and errors always go to stderr, in every mode, so they never land
between the rows a pipe is reading.

### Colour

kasl paints little - clap's help, the `task find` spinner, the interactive
prompts, the debug log - and decides once, for all of them, per stream:

1. `--plain` (or `--json`): no colour.
2. `NO_COLOR` set to anything but an empty string: no colour
   ([no-color.org](https://no-color.org)).
3. `CLICOLOR_FORCE` set to anything but `0`: colour, even into a pipe.
4. `CLICOLOR=0`: no colour.
5. Otherwise: colour only when the stream is a terminal.

## Environment variables

- **`NO_COLOR`**, **`CLICOLOR`**, **`CLICOLOR_FORCE`** - colour, in the order
  [above](#colour).
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
rejected report, a task, tag, template or inbox issue asked for by name that is
not there - is exit code 1, with the reason on stderr. A command line kasl
cannot parse - an unknown flag, `--json` on a command with no data - is exit
code 2, clap's code for usage errors. An empty list is not a failure:
`kasl task list --json` on a day with no tasks prints `{"tasks": []}` and exits
0. A reader that stops reading early (`kasl inbox --plain | head -3`) is not a
failure either: kasl stops quietly with exit code 0. A nightly job, run weekdays at 20:00
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

A machine that reports to a [kasl-server](/reference/server/) can drain its
queue the same way. `kasl server flush` sends whatever is owed and exits 0
when there is nothing waiting, so it is safe to run on a schedule:

```bash
0 * * * * kasl server flush > /dev/null 2>&1
```

The queue is what makes that unattended job harmless: a run while the server
is down leaves the days owed rather than losing them, and the next run sends
them.

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
