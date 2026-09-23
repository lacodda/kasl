---
title: "inbox"
---

The `inbox` command manages a local inbox of open Jira issues assigned to you. The watcher polls Jira in the background, stores discovered issues locally, and shows a desktop toast when a new issue appears or an existing one visibly changes. From the inbox you can pin, snooze, dismiss, open in the browser, or take an issue into your task list - one at a time, or the whole pile in one sitting with `triage`.

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

### `triage` - Triage the inbox issue by issue

```bash
kasl inbox triage [--snooze-for <FOR>] [FILTERS]
```

**Options:**
- `--snooze-for <FOR>`: How long `snooze` sleeps during this run - `3d`, `12h`, `2w`. Defaults to `1d`.
- The [filters](#filters) above, so `triage --since 7d --min-score 5` walks exactly that slice

Walks the inbox one issue at a time and asks what to do with each. Two hundred issues are not triaged by running `take`, `snooze` and `dismiss` two hundred times, each re-reading the list to find the next row.

Each issue is shown with its position, badge, score and priority, and offers:

- **take** - make it a task and start
- **snooze** - not now, bring it back later (for `--snooze-for`)
- **dismiss** - not mine, hide it for good
- **open** - look at it in the browser, then decide
- **skip** - leave it and move on (the default)
- **quit** - stop here; everything not yet decided is left untouched

`open` is a look, not a decision: the issue is asked about again once the browser is up. `skip` leads because the common answer in a long run is "not this one", and it is the only choice that changes nothing if the finger slips. A run ends with what it came to:

```
[✓] Triaged: 3 taken, 5 snoozed, 2 dismissed, 12 skipped.
```

Triage is a conversation, so it needs a terminal. In a script, use `inbox take`, `inbox snooze` and `inbox dismiss` with explicit keys.

### `show` - Show one inbox issue

```bash
kasl inbox show [KEY] [--why] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

**Options:**
- `--why`: Explain where the ranking comes from and what each mark means

One issue in full: the whole summary, the URL, and the dates a six-column table row has no room for.

`--why` answers the question a list sorted by an invisible field invites - why is this issue *here*:

```
+----------+--------------+---------------------------------------------------+
| WHAT     | VALUE        | WHY                                               |
+----------+--------------+---------------------------------------------------+
| score    | 8            | read from Scoring in Jira; the list is ordered by  |
|          |              | it, highest first                                 |
| priority | High         | Jira priority id 2, which breaks ties on equal     |
|          |              | scores - lower is more urgent                     |
| pinned   | yes          | pinned issues lead the list whatever the order     |
+----------+--------------+---------------------------------------------------+
```

Note what this is not: a score broken into points. kasl does not compute importance. The score is a number read straight out of the Jira field you named in [configuration](#configuration), and the priority rank is Jira's own priority id - so `--why` names where each part came from rather than inventing a local formula that would disagree with Jira about which issue matters.

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

## Toast buttons

A toast about one issue carries the three decisions worth making about it, so the pile can be triaged without opening a terminal:

- **Take** - imports the issue into your tasks, exactly as `kasl inbox take` does
- **Snooze** - puts it to sleep for `toast_snooze_for` (a day by default)
- **Dismiss** - hides it from the list

Clicking the toast *body*, as before, opens the issue in the browser.

The press is carried out by the running `kasl watch` daemon, which answers with a second toast saying what happened - `KA-1 is now a task`, `KA-1 sleeps until Sep 18 09:30`. An answer within a couple of seconds is the point: a button that changes something in silence leaves you unsure the press registered.

Pressing a button for an issue that Jira has since closed or reassigned says so (`KA-1 is no longer in the inbox`) instead of failing quietly. Pressing **Take** twice reports the task that already exists rather than making a second copy - which matters, because a toast can be pressed from the notification centre long after it appeared.

Stopping the watcher does not leave a dead button: with no daemon running, the press is carried out on the spot instead.

### What each platform can do

| Platform | Toast body | Buttons |
| --- | --- | --- |
| Windows | Opens the issue | **Take**, **Snooze**, **Dismiss** |
| Linux (and other XDG desktops) | Opens the issue | **Take**, **Snooze**, **Dismiss** |
| macOS | Display only | None - use `kasl inbox triage` |

On macOS the notification API cannot report a click at all, so the buttons are not drawn rather than drawn dead: `kasl inbox triage` walks the same three decisions with room to think.

Windows needs no registry entry and no protocol of its own. A toast button there can only ask the shell to launch a URI, and no argument survives that trip, so the watcher writes one shortcut per (action, issue) under `toast-buttons/` in the data directory and the button launches that. The shortcuts of a decided issue are removed once it is settled.

## Background Polling

Polling runs inside `kasl watch` (both daemon and `--foreground` modes). New issues trigger a desktop notification; clicking the toast opens the issue in the browser on Windows and Linux, and its buttons decide it. On macOS the toast is display-only - the notification API cannot report a click - so opening stays on `kasl inbox open`. Each issue is notified about only once. Status and priority changes on existing issues also toast (a score change or a return after a missed poll only sets the badge), and issues leaving the inbox can toast too when `notify_gone` is enabled. Snoozed issues whose time is up are woken at the start of each poll and toast their return; that step is local and runs even when the Jira poll itself is skipped or fails.

A poll that fails - VPN down, Jira unreachable, a session that could not be renewed - changes nothing: the list is reconciled only against an answer Jira actually gave, so a bad night does not mark every issue `gone` and bring all of them `back` in the morning. Two answers that look like answers are refused the same way: an expired session that Jira serves as the anonymous user (an empty page with status 200), and a list shorter than the total Jira counted, which happens when an issue is created or resolved while the pages are read.

Toasts are budgeted over time, not per poll. Within any hour at most five single toasts show; past that, one summary toast says how many issues were new, changed or left and opens your open-issues list in Jira, and the rest of the hour stays quiet. A trickle of three changes every five minutes is held to the same bound as two hundred at once.

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
    "toast_snooze_for": "1d",
    "custom_fields": [{ "id": "customfield_12345", "label": "Scoring" }],
    "sort_by_field": "customfield_12345"
  }
}
```

- `enabled`: Whether the watcher polls Jira (default `true` when the section is present)
- `poll_interval_secs`: Seconds between polls (default `300`)
- `notify`: Show desktop toasts for new issues (default `true`); when `false`, all inbox toasts are off
- `notify_changes`: Toast when an existing issue changes status or priority (default `true`); score changes only set the badge
- `notify_gone`: Toast when an issue leaves the inbox — closed or reassigned (default `false`)
- `toast_snooze_for`: How long a toast's **Snooze** button sleeps — `3d`, `12h`, `2w` (default `1d`), the same spelling as `triage --snooze-for`
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

# Clear the whole pile in one sitting
kasl inbox triage

# Triage only this week's serious ones, sleeping the rest for three days
kasl inbox triage --since 7d --min-score 5 --snooze-for 3d

# Put one issue down until Thursday, and see what is asleep
kasl inbox snooze PROJ-123 3d
kasl inbox --snoozed

# Why is this issue at the top?
kasl inbox show PROJ-123 --why

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
