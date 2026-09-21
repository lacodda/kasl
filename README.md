<p align="center"><img src="https://github.com/lacodda/kasl/raw/main/assets/banner.svg" alt="kasl - key activity synchronization and logging" width="720"></p>

> Your workday, recorded while you work: kasl watches activity in the background, turns it into intervals, pauses and tasks, and files the report for you.

<p align="center">
  <a href="https://crates.io/crates/kasl-cli"><img src="https://img.shields.io/crates/v/kasl-cli?style=flat-square" alt="crates.io"></a>
  <a href="https://www.npmjs.com/package/kasl-cli"><img src="https://img.shields.io/npm/v/kasl-cli?style=flat-square" alt="npm"></a>
  <a href="https://github.com/lacodda/kasl/actions"><img src="https://img.shields.io/github/actions/workflow/status/lacodda/kasl/ci.yml?style=flat-square" alt="CI"></a>
  <a href="https://github.com/lacodda/kasl/blob/main/LICENSE"><img src="https://img.shields.io/github/license/lacodda/kasl?style=flat-square" alt="License"></a>
</p>

## Why

Time sheets get filled in from memory, at the end of the day, when the day is already gone. You reconstruct when you started, guess how long lunch was, and try to recall what that morning hour went into.

kasl records it as it happens. A background daemon watches keyboard and mouse activity, decides when the workday started, notices the breaks, and keeps the intervals. Tasks come from your own commits and issues rather than from memory. At the end you look at the day and send it, instead of inventing it.

## A day in the life

Start the daemon once - or have it start itself at login - and forget about it:

```console
$ kasl watch
Watcher started in the background (PID: 24180).
```

Later, note what you worked on. Candidates come from today's GitLab commits and resolved Jira issues, so most of this is picking from a list rather than typing:

```console
$ kasl task find
Found: 1 incomplete, 2 jira, 4 gitlab
? Select tasks to import ›
❯ ◉ ↻ PROJ-419 Draft migration for protected pauses — 60%
  ◉ ◉ PROJ-412 Fix session timeout on the settings page
  ◯ ● Review PR #318: pause merging (a1c9f42)
```

Look at the day. The intervals, the breaks and the productivity figure were recorded while you worked:

```console
$ kasl report
Report for August 8, 2026

+--------------+-------+-------+----------+
| ID           | START | END   | DURATION |
+--------------+-------+-------+----------+
| 1            | 09:12 | 13:30 | 04:18    |
| 2            | 14:18 | 16:02 | 01:44    |
| 3            | 16:29 | 18:04 | 01:35    |
|              |       |       |          |
| TOTAL        |       |       | 07:37    |
| PRODUCTIVITY |       |       | 96.1%    |
+--------------+-------+-------+----------+

Tasks:

+---+----+---------------------------------------------------+------------------+------+
| # | ID | NAME                                              | COMMENT          | DONE |
+---+----+---------------------------------------------------+------------------+------+
| 1 | 1  | PROJ-412 Fix session timeout on the settings page | stale cookie jar | 100% |
| 2 | 2  | Review PR #318: pause merging                     |                  | 100% |
| 3 | 3  | PROJ-419 Draft migration for protected pauses     | backfill pending | 60%  |
+---+----+---------------------------------------------------+------------------+------+
```

The monitor only sees the keyboard and the mouse, so an hour in a meeting room leaves no trace. Put it on the record yourself:

```console
$ kasl pauses add --start 15:00 --minutes 40 --reason "offsite meeting"
Pause recorded: 15:00 - 15:40 (40 minutes)
```

Send the day when it is done:

```console
$ kasl report --send
Your report dated August 8, 2026 has been successfully submitted
Wait for a message to your email address
```

And watch the month accumulate:

```console
$ kasl sum
Working hours for August, 2026

+------------+-------+--------------+
| DATE       | HOURS | PRODUCTIVITY |
+------------+-------+--------------+
| 2026-08-07 | 08:04 | 94.7%        |
| 2026-08-08 | 07:37 | 96.1%        |
|            |       |              |
| TOTAL      | 15:41 |              |
| AVERAGE    | 07:50 |              |
+------------+-------+--------------+

Monthly work productivity: 95.4%
```

## What you get

- **A workday that records itself.** The daemon starts the day on sustained
  activity rather than the first stray keypress, and closes pauses when you
  come back, so the productivity figure means something.
- **Tasks you do not have to remember.** Today's GitLab commits and resolved
  Jira issues arrive as candidates, deduplicated against what you already
  logged; `kasl inbox triage` walks a slice of the backlog issue by issue, and
  snoozing puts one down until a date and brings it back by itself.
- **A toast you can answer.** A notification about a Jira issue carries Take,
  Snooze and Dismiss, so most of the backlog is triaged without opening a
  terminal. Windows and Linux; on macOS the notification API cannot report a
  click, so the buttons are left out rather than drawn dead.
- **Honest numbers.** kasl records absences rather than inventing them, and
  says so when a day falls below your reporting threshold.
- **Reports where they need to go.** One command files the day or the month to
  your corporate API; exports to CSV, JSON and Excel, hourly breakdown
  included.
- **What leaves the machine, shown before it does.** `kasl server manifest`
  prints what the team server stores about you, in that installation's own
  words, read from the server rather than described here;
  `kasl report --send --show` prints the payload the corporate API would
  receive, field by field, and sends nothing.
- **Passwords in the OS keyring** - Credential Manager, Keychain, Secret
  Service. Nothing is encrypted with a key that ships inside the binary.
- **Nothing that hangs.** Every prompt checks for a terminal first, so kasl
  under cron or under the daemon fails with the flag you needed instead of
  waiting for an answer nobody can give.
- **A team server, when there is one.** `kasl server connect` points this
  machine at a self-hosted
  [kasl-server](https://github.com/lacodda/kasl-server); a day that cannot be
  delivered is queued, and the next push carries the backlog with it.
  Connecting is optional and changes nothing locally.
- **A short alias.** `ka` is installed beside `kasl` as a *link* to the same
  binary, so an update can never leave one behind. Completions for bash, zsh,
  fish, PowerShell and elvish.

## Install

**With npm:**

```bash
npm i -g kasl-cli
```

**With cargo:**

```bash
cargo install kasl-cli
```

**One-line installers.** Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.ps1 | iex
```

macOS / Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.sh | sh
```

On Windows use the PowerShell line above: `install.sh` carries the macOS and Linux builds only, and run from Git Bash it stops with a pointer back here.

**Binary releases** - grab the archive for your platform from [Releases](https://github.com/lacodda/kasl/releases/latest) (Windows x86_64, Linux x86_64, macOS arm64), unpack and put `kasl` on your `PATH`. The archive carries one binary; `ka` is a link the installers create beside it, and you can make one yourself the same way.

Both installers take the newest release by default; set `KASL_VERSION` to a tag to pin one, `KASL_INSTALL_DIR` to choose where the binary lands, and `KASL_NO_ALIAS=1` to skip the `ka` alias.

## Quick start

```bash
kasl setup                # first-run wizard: monitor settings, integrations, credentials
kasl watch                # start monitoring in the background
kasl autostart enable     # and have it start at login
kasl task find            # pick up today's commits and issues
kasl report               # see the day
kasl report --send        # file it
```

Data lives in the platform user data directory: `%LOCALAPPDATA%\lacodda\kasl` on Windows, `~/Library/Application Support/lacodda/kasl` on macOS, `~/.local/share/lacodda/kasl` on Linux.

Full command reference and concepts: **[kasl.lacodda.com](https://kasl.lacodda.com)**.

## Status

In daily use on Windows, macOS and Linux. Everything above works today: the
daemon, the Jira and GitLab integrations, the reports and the optional team
server. The current version and what landed in it:
[CHANGELOG](https://github.com/lacodda/kasl/blob/main/CHANGELOG.md).

## Documentation

The documentation site (Astro Starlight) lives in [`docs/`](https://github.com/lacodda/kasl/tree/main/docs); architecture decision records are in [`docs/adr/`](https://github.com/lacodda/kasl/tree/main/docs/adr).

## License

MIT (c) [Kirill Lakhtachev](https://lacodda.com)
