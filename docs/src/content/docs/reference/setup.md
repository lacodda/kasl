---
title: "setup"
---

The `setup` command runs the interactive configuration wizard: a checklist of modules, then a few prompts for each one you ticked.

:::note[Renamed in 1.2]
This command used to be called `init`. The old name still works and does the
same thing, printing a notice that points here; it will be removed in 2.0.
:::

`kasl setup` needs a terminal - every step is a prompt, several of them for secrets. Run it outside one and it refuses with a message naming the problem rather than hanging.

## Usage

```bash
kasl setup [OPTIONS]
```

- `-d, --delete`: Remove the existing configuration file and global PATH settings instead of creating a new one.

## Module checklist

The wizard opens with a single checklist - a `MultiSelect`, not a chain of "Enable X? (y/N)" questions - listing every module in this order:

1. **SiServer**
2. **GitLab**
3. **Jira**
4. **Monitor**
5. **Server**
6. **Productivity**
7. **Report**
8. **Task discovery**
9. **Jira inbox**

Whatever you tick runs its own prompts right after the checklist closes; nothing you leave unticked is touched. Fields already in your config file are offered back as defaults, so re-running `setup` on a module you configured before is an edit, not a rewrite.

### SiServer, GitLab, Jira

Each asks for a login/token and its API URL:

- **SiServer**: login, login URL, API URL.
- **GitLab**: private token, API URL.
- **Jira**: login, API URL - just the two fields; there is no separate "Jira Username" prompt.

### Monitor

Prompts for `min_pause_duration`, `pause_threshold`, `poll_interval`, `activity_threshold`, and `min_work_interval`. One monitor setting, `pause_merge_gap`, is not part of this prompt set - it only has a default and is changed by editing `config.json` directly. See [Configuration](/concepts/configuration/) for what each field controls.

### Server

Prompts for the reporting API URL and auth token used by `report --send` and `sum --send`.

### Productivity, Report, Task discovery, Jira inbox

The remaining four modules configure the productivity thresholds, report output (directory, filename template, language, template name), the task-discovery ignore list, and Jira inbox polling (interval, notifications, sort field, custom fields) respectively. Field-by-field details live on [Configuration](/concepts/configuration/) - this page only tracks what the wizard asks and in what order.

## Configuration file location

- **Windows**: `%LOCALAPPDATA%\lacodda\kasl\config.json`
- **macOS**: `~/Library/Application Support/lacodda/kasl/config.json`
- **Linux**: `~/.local/share/lacodda/kasl/config.json`

## Examples

```bash
# Run the wizard
kasl setup

# Remove the configuration and start over
kasl setup --delete
kasl setup
```

## Related commands

- [`watch`](/reference/watch/) - Start activity monitoring
- [`task`](/reference/task/) - Manage tasks
- [`report`](/reference/report/) - Generate reports
