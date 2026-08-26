---
title: "task"
---

The `task` command creates, lists, shows, edits and removes tasks, and can pull candidates from Jira and GitLab.

## Usage

```bash
kasl task [COMMAND]
```

Running `kasl task` without a subcommand creates a task interactively.

## Commands

### `add` - Add a task

```bash
kasl task add [OPTIONS]
```

- `-n, --name <NAME>`: Task name. Required for non-interactive creation.
- `--comment <COMMENT>`: Task comment or description.
- `-c, --completeness <COMPLETENESS>`: Completion percentage (0-100).
- `--tags <TAGS>`: Comma-separated tags to assign. Tags are created automatically if they do not exist yet. This is the only place tags are set - `task edit` never touches them.
- `-t, --template <TEMPLATE>`: Create from a named template.
- `-l, --from-template`: Pick a template interactively.

Without `--name` (or a full set of non-interactive options), `kasl task add` prompts interactively. Outside an interactive terminal it errors instead of hanging: "task name is required; pass --name outside an interactive terminal" - so it's safe to call from scripts as long as `--name` is provided.

#### `--template` and the flag overrides

`--template NAME` fills the task from a saved template, but any of `--name`, `--comment`, `--completeness` or `--tags` given alongside it wins over the template's value for that field - the template supplies defaults, not fixed values. Fields you don't override, and don't have a flag for, fall back to the template outside a terminal; in a terminal they are offered as prompts seeded with the template's value. `--tags` is independent of the template either way: templates carry no tags, so tagging a templated task always goes through `--tags`.

### `list` - List tasks

```bash
kasl task list [OPTIONS]
```

- `-a, --all`: List tasks from every date, not just today.
- `--tag <TAG>`: Only tasks carrying this tag.

### `show` - Show tasks by id

```bash
kasl task show [ID]...
```

- `ID...`: One or more task ids to show. Omit them on a terminal to pick from today's tasks.

### `edit` - Edit a task by id, or several interactively

```bash
kasl task edit [ID]
```

- `[ID]`: Task id to edit; omit to pick several interactively.

Editing only ever touches name, comment and completeness - tags are left as they are; assign them with `task add --tags` instead.

### `remove` - Remove tasks by id, or all of today's

```bash
kasl task remove [OPTIONS] [ID]...
```

- `[ID]...`: Task ids to remove.
- `--today`: Remove every task recorded for today.
- `-y, --yes`: Remove without asking for confirmation.

### `find` - Find incomplete tasks and import from GitLab/Jira

```bash
kasl task find
```

- Shows a spinner while searching incomplete local tasks, today's Jira issues, and GitLab commits.
- Presents a single consolidated MultiSelect (incomplete tasks first, then a separator, then Jira/GitLab).
- Filters out tasks already logged today, near-duplicate names, and names from `task_discovery.ignore_names`.
- After import selection, optionally add items to the persistent ignore list.
- Selected incomplete tasks prompt for an updated completeness percentage before insert.

## Examples

```bash
# Create a new task
kasl task add --name "New Task" --comment "This is a test task" --completeness 50

# Create task with tags
kasl task add --name "Fix bug" --tags "urgent,backend" --completeness 0

# Display today's tasks
kasl task list

# Display all tasks
kasl task list --all

# Display tasks with specific tag
kasl task list --tag "urgent"

# Find and import incomplete/Jira/GitLab tasks
kasl task find

# Show specific tasks
kasl task show 1

# Edit a specific task
kasl task edit 1

# Edit several tasks interactively
kasl task edit

# Create task from template, picked interactively
kasl task add --from-template

# Use a named template, non-interactively
kasl task add --template "daily-standup"

# Use a template but override its name and add tags
kasl task add --template "daily-standup" --name "Standup - sprint planning" --tags "meeting"

# Remove specific tasks
kasl task remove 1 2 3

# Remove specific tasks without confirmation
kasl task remove 1 2 3 -y

# Remove all today's tasks without confirmation
kasl task remove --today -y
```

## Sample Output

Running `kasl task list` after adding one task:

```
+---+----+---------+---------------+------+
| # | ID | TASK ID | NAME          | DONE |
+---+----+---------+---------------+------+
| 1 | 1  | 1       | Review PR 318 | 100% |
+---+----+---------+---------------+------+
```

The `TASK ID`, `COMMENT` and `TAGS` columns only appear when at least one listed task actually has that data - an all-local, comment-less, tag-less list renders as just `#`, `ID`, `NAME`, `DONE`.

## Related commands

- [`tag`](/reference/tag/) - Manage tags for task categorization
- [`template`](/reference/template/) - Create and use task templates
- [`report`](/reference/report/) - View tasks in work reports
- [`export`](/reference/export/) - Export task data for analysis
