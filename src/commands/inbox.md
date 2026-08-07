# `inbox` Command

The `inbox` command manages a local inbox of open Jira issues assigned to you. The watcher polls Jira in the background, stores discovered issues locally, and shows a desktop toast when a new issue appears. From the inbox you can pin, dismiss, open in the browser, or import issues into your local task list.

## Usage

```bash
kasl inbox [OPTIONS]
```

Running `kasl inbox` without options lists the active (non-dismissed) issues.

## Options

- `-l, --list`: List active inbox issues (default action)
- `-n, --limit <N>`: Show only the top N issues; the list is sorted by pin state, ranking field (e.g. Scoring), and priority
- `--sync`: Poll Jira immediately instead of waiting for the background cadence
- `--pin <KEY>`: Pin an issue (pinned issues stay on top)
- `--unpin <KEY>`: Remove the pin
- `--dismiss <KEY>`: Hide an issue from the list
- `--open <KEY>`: Open the issue in the browser
- `--take <KEY>`: Import the issue into local tasks (creates a task named `KEY summary` and dismisses the inbox entry)

## Background Polling

Polling runs inside `kasl watch` (both daemon and `--foreground` modes). New issues trigger a desktop notification; clicking the toast opens the issue in the browser (Windows). Each issue is notified about only once.

## Configuration

Polling is enabled by adding the `jira_inbox` section to the config; the `jira` section must be configured as well.

```json
{
  "jira_inbox": {
    "enabled": true,
    "poll_interval_secs": 300,
    "notify": true,
    "custom_fields": [{ "id": "customfield_12345", "label": "Scoring" }],
    "sort_by_field": "customfield_12345"
  }
}
```

- `enabled`: Whether the watcher polls Jira (default `true` when the section is present)
- `poll_interval_secs`: Seconds between polls (default `300`)
- `notify`: Show desktop toasts for new issues (default `true`)
- `custom_fields`: Extra Jira fields to fetch and display, such as a Scoring field
- `sort_by_field`: Field id used to rank the list in descending order

## Examples

```bash
# Show the inbox
kasl inbox

# Top five issues by ranking
kasl inbox -n 5

# Sync now and show the result
kasl inbox --sync --list

# Work with a specific issue
kasl inbox --pin PROJ-123
kasl inbox --open PROJ-123
kasl inbox --take PROJ-123
```
