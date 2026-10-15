---
title: "tag"
---

The `tag` command manages tags: short, colored labels you attach to tasks for categorization and filtering.

## Usage

```bash
kasl tag [COMMAND]
```

| Subcommand | Purpose |
| --- | --- |
| `add` | Add a new tag |
| `list` | List all available tags |
| `show` | Show a tag and the tasks that carry it |
| `edit` | Edit an existing tag's name/color |
| `remove` | Remove a tag and unassign it from all tasks |

## `kasl tag add`

```bash
kasl tag add <NAME> [OPTIONS]
```

- `NAME`: unique name for the tag. Required.
- `-c, --color <COLOR>`: optional color - a common name ("red", "blue", "green", "yellow", "purple", "orange") or a hex code ("#FF6B6B").

```bash
kasl tag add "urgent" --color red
kasl tag add "frontend" --color "#FF6B6B"
```

Tags are also created implicitly: `kasl task add --tags "new-tag,urgent"` creates any tag in the list that does not already exist.

## `kasl tag list`

```bash
kasl tag list [--json]
```

Displays a table of all tags with their ID, name, and color.

- `--json`: Print them as one JSON document - see [JSON output](#json-output).

## `kasl tag show`

```bash
kasl tag show <TAG> [--json]
```

- `TAG`: tag name or ID. Required.
- `--json`: Print the tag and its tasks as one JSON document - see [JSON output](#json-output).

Shows the tag and every task currently assigned to it. A tag that does not exist is an error (exit code 1), so a script can tell it from a tag that carries no tasks.

## `kasl tag edit`

```bash
kasl tag edit [TAG]
```

- `TAG`: tag name or ID to edit. Omit it on a terminal to pick from the list.

Prompts interactively for a new name and color. This command is interactive only - it has no flags for scripting.

## `kasl tag remove`

```bash
kasl tag remove [TAG] [OPTIONS]
```

- `TAG`: tag name or ID to remove. Omit it on a terminal to pick from the list.
- `-y, --yes`: remove without asking for confirmation.

Removing a tag unassigns it from every task that carries it; the tasks themselves are untouched.

## Assigning tags to tasks

Tags are assigned only when a task is created, via `kasl task add --tags`:

```bash
kasl task add --name "Fix login bug" --tags "urgent,bug"
```

There is no way to add or remove tags on an existing task - `kasl task edit` only changes name, comment, and completeness.

## Filtering by tag

```bash
kasl task list --tag "urgent"
```

## Examples

```bash
# Create a couple of tags
kasl tag add "frontend" --color blue
kasl tag add "urgent" --color red

# List them
kasl tag list

# Create a task carrying both
kasl task add --name "Update UI" --tags "frontend,urgent"

# Filter tasks by tag
kasl task list --tag "urgent"

# Rename a tag interactively
kasl tag edit "urgent"

# Remove a tag without confirmation
kasl tag remove "urgent" -y
```

## Sample Output

```
$ kasl tag add urgent --color red
✅ Tag 'urgent' created successfully.

$ kasl tag list

Tags:

+----+--------+-------+
| ID | NAME   | COLOR |
+----+--------+-------+
| 1  | urgent | red   |
+----+--------+-------+

$ kasl tag remove urgent -y
✅ Tag 'urgent' deleted successfully.
```

`tag edit` prints `✅ Tag '{new-name}' updated successfully.` after its prompts; it has no other output to show statically since the prompts themselves depend on the terminal.

## JSON output

`kasl tag list --json` and `kasl tag show --json` print one document each
(conventions: [kasl in scripts and CI](/guides/scripting/#json-for-programs)):

```json
{
  "tags": [
    { "id": 1, "name": "backend", "color": "blue" },
    { "id": 2, "name": "db", "color": null }
  ]
}
```

```json
{
  "tag": { "id": 2, "name": "db", "color": null },
  "tasks": [
    {
      "id": 3,
      "chain_id": 3,
      "created_at": "2026-08-08T10:05:41",
      "name": "PROJ-419 Draft migration for protected pauses",
      "comment": "backfill pending",
      "completeness": 60,
      "tags": ["backend", "db"],
      "jira_key": null
    }
  ]
}
```

A task has the shape [`task list --json`](/reference/task/#json-output) prints.

## Related commands

- [`task`](/reference/task/) - create and manage tasks, including assigning tags
- [`report`](/reference/report/) - view tasks organized by tags
- [`export`](/reference/export/) - export task data with tag information
