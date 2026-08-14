---
title: "inbox"
---

The `inbox` command manages a local inbox of open Jira issues assigned to you. The watcher polls Jira in the background, stores discovered issues locally, and shows a desktop toast when a new issue appears or an existing one visibly changes. From the inbox you can pin, dismiss, open in the browser, or import issues into your local task list.

Every sync reconciles the list against Jira: issues that stop appearing in the poll (closed or reassigned) are marked gone and leave the list instead of lingering forever. They stay inspectable with `--all`.

## Usage

```bash
kasl inbox [OPTIONS] [COMMAND]
```

Running `kasl inbox` without a subcommand lists the active (non-dismissed) issues.

## Options

- `-n, --limit <N>`: Show only the top N issues; the list is sorted by pin state, ranking field (e.g. Scoring), and priority
- `--all`: Include issues gone from Jira (closed or reassigned); they sort below the present ones

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

The `CHANGE` column carries freshness badges for about a day: `NEW` for freshly discovered issues, a change summary such as `status→In Progress`, `↑prio High`, or `score 5→8` for existing ones, and `gone` for issues no longer returned by Jira (visible only with `--all`).

### `pin` - Pin an inbox issue

```bash
kasl inbox pin [KEY]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox.

Pinned issues stay on top of the list.

### `unpin` - Unpin an inbox issue

```bash
kasl inbox unpin [KEY]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox.

### `dismiss` - Dismiss an inbox issue

```bash
kasl inbox dismiss [KEY]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox.

Hides an issue from the list.

### `open` - Open issue URL in browser

```bash
kasl inbox open [KEY]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox.

### `take` - Import issue into tasks

```bash
kasl inbox take [KEY]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox.

Imports the issue into local tasks (creates a task named `KEY summary` and dismisses the inbox entry).

## Background Polling

Polling runs inside `kasl watch` (both daemon and `--foreground` modes). New issues trigger a desktop notification; clicking the toast opens the issue in the browser (Windows). Each issue is notified about only once. Visible changes to existing issues (status, priority, score) also toast, and issues leaving the inbox can toast too when `notify_gone` is enabled.

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
