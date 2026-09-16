---
title: "inbox"
---

The `inbox` command manages a local inbox of open Jira issues assigned to you. The watcher polls Jira in the background, stores discovered issues locally, and shows a desktop toast when a new issue appears or an existing one visibly changes. From the inbox you can pin, snooze, dismiss, open in the browser, or take an issue into your task list.

Every sync reconciles the list against Jira: issues that stop appearing in the poll (closed or reassigned) are marked gone and leave the list instead of lingering forever. They stay inspectable with `--all`.

## Usage

```bash
kasl inbox [OPTIONS] [COMMAND]
```

Running `kasl inbox` without a subcommand lists the active (non-dismissed) issues.

## Options

- `-n, --limit <N>`: Show only the top N issues after filtering and sorting
- `--all`: Include issues gone from Jira (closed or reassigned); they sort below the present ones
- `--snoozed`: Include issues that are still asleep, each showing the date it is due back

### Filters

An inbox of two hundred open issues is a pile, not a list. These cuts turn it into one; they combine (every set filter must hold) and apply to the bare `kasl inbox`, to `list`, and to every picker (`take`, `open`, `pin`, `unpin`, `dismiss` without a key), so a `take --since 7d` offers only this week's issues.

- `--since <WINDOW>`: Only issues first seen within the window - `1d`, `7d`, `12h`, `2w`; a bare number is days
- `--new`: Only issues discovered in the last day (the same window as the `NEW` badge)
- `--changed <WINDOW>`: Only issues that visibly changed (status, priority, score) within the window; issues that never changed are out
- `--min-score <N>`: Only issues whose ranking field (e.g. Scoring) is at least N; issues without a score are out
- `--priority <NAME[+]>`: Only issues of that priority, named as your Jira names it; add `+` for that priority and everything more urgent (`High+`). An unknown name lists the ones the inbox knows
- `--status <NAME>`: Only issues in that status, by name or id, regardless of case
- `--sort <score|priority|new|changed>`: Order of the list; `score` is the inbox's own order (ranking field, then priority). Pinned issues lead and gone issues trail whatever the order

A cut list says so in its header - `Jira inbox: 12 of 200 issues (since 7d, score ≥ 5):` - so a filtered view never reads as the whole inbox. When nothing matches, the message names the cuts and the size of the whole.

## Commands

### `sync` - Sync inbox from Jira

```bash
kasl inbox sync
```

Polls Jira immediately instead of waiting for the background cadence. The summary counts fetched, new, changed, and gone issues:

```
[✓] Jira inbox synced: 7 fetched, 1 new, 2 changed, 1 gone.
```

### `list` - List active inbox issues

```bash
kasl inbox list [OPTIONS]
```

**Options:**
- `-n, --limit <N>`: Show only the top N issues
- `--all`: Include issues gone from Jira
- `--snoozed`: Include issues that are still asleep
- The [filters](#filters) above: `--since`, `--new`, `--changed`, `--min-score`, `--priority`, `--status`, `--sort`

The `CHANGE` column carries freshness badges for about a day: `NEW` for freshly discovered issues, a change summary such as `status→In Progress`, `↑prio High`, or `score 5→8` for existing ones, and `gone` for issues no longer returned by Jira (visible only with `--all`). `taken` marks an issue you have already started; unlike the others it does not fade, and it outranks `NEW` and change summaries. `zzz Mar 4` is a sleeping issue and the date it is due back, shown only under `--snoozed`; `back` marks one whose snooze has just run out. `gone` outranks everything.

### `pin` - Pin an inbox issue

```bash
kasl inbox pin [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

Pinned issues stay on top of the list.

### `unpin` - Unpin an inbox issue

```bash
kasl inbox unpin [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

### `dismiss` - Dismiss an inbox issue

```bash
kasl inbox dismiss [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

Hides an issue from the list, for good. Dismissal is the answer to "this is
not mine"; for "not now", see [`snooze`](#snooze---snooze-an-inbox-issue).

### `snooze` - Snooze an inbox issue

```bash
kasl inbox snooze [KEY] [FOR] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.
- `FOR`: How long to sleep - `3d`, `12h`, `2w`; a bare number is days. Defaults to `1d`.

Puts an issue down until the moment passes. It leaves the list, stops counting
among what is waiting, and comes back on its own when its time is up - with a
toast and a `back` badge saying why it has returned.

The difference from `dismiss` is the returning. Dismissal means "never" and is
right for an issue that is not yours; snoozing means "not now" and is right for
the issue you will deal with on Monday. Without it the only way to defer an
issue was to keep reading past it, which is how an inbox stops being read.

Sleeping issues come back whether or not Jira is reachable: the due date is
local bookkeeping, so a VPN outage cannot hold an issue past its moment.

```
[✓] Snoozed PROJ-123 until Mar 4 09:30.
```

### `unsnooze` - Wake a snoozed inbox issue

```bash
kasl inbox unsnooze [KEY]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the sleeping issues - only those, since waking an awake issue does nothing.

Brings a sleeping issue back before its time.

### `open` - Open issue URL in browser

```bash
kasl inbox open [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

### `take` - Start working on an issue

```bash
kasl inbox take [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

Creates a task named `KEY summary` and records that the issue is in hand: the
task stores the issue key, and the issue stays in the inbox wearing a `taken`
badge. Dismissing is still a separate act - `take` means "I started this", not
"this is not mine".

Taking the same issue twice does not create a second task; the command says
which task it already became. The stored key survives renaming the task, so the
link outlives the summary it started with.

## Background Polling

Polling runs inside `kasl watch` (both daemon and `--foreground` modes). New issues trigger a desktop notification; clicking the toast opens the issue in the browser on Windows and Linux. On macOS the toast is display-only - the notification API cannot report a click - so opening stays on `kasl inbox open`. Each issue is notified about only once. Visible changes to existing issues (status, priority, score) also toast, and issues leaving the inbox can toast too when `notify_gone` is enabled. Snoozed issues whose time is up are woken at the start of each poll and toast their return; that step is local and runs even when the Jira poll itself is skipped or fails.

A poll that fails - VPN down, Jira unreachable, a session that could not be renewed - changes nothing: the list is reconciled only against an answer Jira actually gave, so a bad night does not mark every issue `gone` and bring all of them `back` in the morning. And when one poll would raise more than five toasts of a kind (a first sync of a long backlog, a Jira-side re-scoring), they collapse into a single summary toast that opens your open-issues list in Jira.

## Configuration

Polling is enabled by adding the `jira_inbox` section to the config; the `jira` section must be configured as well.

```json
{
  "jira_inbox": {
    "enabled": true,
    "poll_interval_secs": 300,
    "notify": true,
    "notify_changes": true,
    "notify_gone": false,
    "custom_fields": [{ "id": "customfield_12345", "label": "Scoring" }],
    "sort_by_field": "customfield_12345"
  }
}
```

- `enabled`: Whether the watcher polls Jira (default `true` when the section is present)
- `poll_interval_secs`: Seconds between polls (default `300`)
- `notify`: Show desktop toasts for new issues (default `true`); when `false`, all inbox toasts are off
- `notify_changes`: Toast when an existing issue changes status, priority, or score (default `true`)
- `notify_gone`: Toast when an issue leaves the inbox — closed or reassigned (default `false`)
- `custom_fields`: Extra Jira fields to fetch and display, such as a Scoring field
- `sort_by_field`: Field id used to rank the list in descending order

## Examples

```bash
# Show the inbox
kasl inbox

# Top five issues by ranking
kasl inbox -n 5

# This week's arrivals, most urgent first
kasl inbox --since 7d --sort priority

# What is worth a look: High or above, scoring at least 5
kasl inbox --priority High+ --min-score 5

# Pick something to start from what changed today
kasl inbox take --changed 1d

# Sync now and show the result
kasl inbox sync
kasl inbox list

# Check what left the inbox
kasl inbox list --all

# Work with a specific issue
kasl inbox pin PROJ-123
kasl inbox open PROJ-123
kasl inbox take PROJ-123
```

## Scripting

Every subcommand takes its issue key as an argument, so the inbox can be driven from scripts without any interactive prompt:

```bash
kasl inbox sync
kasl inbox take PROJ-123
kasl inbox dismiss PROJ-456
```

## Related commands

- [`watch`](/reference/watch/) - the daemon that polls the inbox
- [`task`](/reference/task/) - where `take` puts the issue
- [`setup`](/reference/setup/) - configuring the Jira connection
- [Configuration](/concepts/configuration/) - the `jira_inbox` block in full
