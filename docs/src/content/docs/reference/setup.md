---
title: "setup"
---

The `setup` command runs the interactive configuration wizard: a checklist of modules, then a few prompts for each one you ticked.

:::note[Renamed in 1.2]
This command used to be called `init`. The old name still works and does the
same thing, printing a notice that points here; it will be removed in 2.0.
:::

`kasl setup` needs a terminal - every step is a prompt, several of them for secrets. Run it outside one and it refuses with a message naming the problem rather than hanging, before it touches a running watcher. To change one setting - from a script, or without walking a module - use [`kasl config set`](/reference/config/).

## Usage

```bash
kasl setup [OPTIONS]
```

- `-d, --delete`: Delete the configuration file instead of running the wizard, so every setting is back to its default. The database, the OS keyring and the PATH are left as they are; it needs no terminal.

## Module checklist

The wizard opens with a single checklist - a `MultiSelect`, not a chain of "Enable X? (y/N)" questions - listing every module in this order:

1. **SiServer**
2. **Jira**
3. **Monitor**
4. **Productivity**
5. **Report**
6. **Task discovery**
7. **Inbox**

Whatever you tick runs its own prompts right after the checklist closes; nothing you leave unticked is touched. Fields already in your config file are offered back as defaults, so re-running `setup` on a module you configured before is an edit, not a rewrite.

### SiServer, Jira

Each asks for a login and its API URL:

- **SiServer**: login, login URL, API URL.
- **Jira**: login, API URL - just the two fields; there is no separate "Jira Username" prompt.

GitLab is not a wizard module. It is a plugin that ships with kasl, and it is set up with `kasl plugin setup gitlab` - see the [plugin reference](/reference/plugin/).

### Monitor

Prompts for `min_pause_duration`, `pause_threshold`, `poll_interval`, `activity_threshold`, and `min_work_interval`. One monitor setting, `pause_merge_gap`, is not part of this prompt set - it is changed with `kasl config set monitor.pause_merge_gap 45`. See [Configuration](/concepts/configuration/) for what each field controls.

### Productivity, Report, Task discovery, Inbox

The remaining four modules configure the productivity thresholds, report output (directory, filename template, language, template name), the task-discovery ignore list, and Jira inbox polling (interval, notifications, sort field, custom fields) respectively. Field-by-field details live on [Configuration](/concepts/configuration/) - this page only tracks what the wizard asks and in what order.

## Configuration file location

- **Windows**: `%LOCALAPPDATA%\lacodda\kasl\config.json`
- **macOS**: `~/Library/Application Support/lacodda/kasl/config.json`
- **Linux**: `~/.local/share/lacodda/kasl/config.json`

## Examples

```bash
# Run the wizard
kasl setup

# Delete the configuration file and start over
kasl setup --delete
kasl setup

# Change one setting without the wizard
kasl config set monitor.pause_threshold 90
```

## Related commands

- [`config`](/reference/config/) - Read and change one setting at a time
- [`watch`](/reference/watch/) - Start activity monitoring
- [`task`](/reference/task/) - Manage tasks
- [`report`](/reference/report/) - Generate reports
