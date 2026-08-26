---
title: "template"
---

The `template` command manages reusable task templates: predefined name, comment, and completeness values you can apply when creating a task instead of typing them each time.

## Usage

```bash
kasl template [COMMAND]
```

| Subcommand | Purpose |
| --- | --- |
| `add` | Create a new template |
| `list` | List all templates |
| `show` | Show one template's contents |
| `edit` | Edit an existing template |
| `remove` | Remove a template |
| `search` | Search templates by name or task name |

## `kasl template add`

```bash
kasl template add [OPTIONS]
```

- `-n, --name <NAME>`: Unique name identifying the template.
- `-t, --task-name <TASK_NAME>`: Task name the template fills in.
- `--comment <COMMENT>`: Comment the template fills in.
- `-c, --completeness <COMPLETENESS>`: Default completion percentage (0-100).

Supplying `--name` together with `--task-name` is what makes template creation scriptable - with both given, the remaining fields fall back to an empty comment and 100% completeness rather than prompting. With any field omitted in a terminal, the command prompts for it.

```bash
# Fully scripted, no prompts
kasl template add --name review --task-name "Code review" --comment daily --completeness 80

# Interactive - prompts for whatever is not passed
kasl template add --name daily-standup
```

## `kasl template list`

```bash
kasl template list
```

Prints a table of every template: name, task name, comment, and completeness.

## `kasl template show`

```bash
kasl template show [NAME]
```

- `NAME`: Template to show. Omit it in a terminal to pick from a list.

## `kasl template edit`

```bash
kasl template edit [NAME]
```

- `NAME`: Template to edit. Omit it in a terminal to pick from a list.

## `kasl template remove`

```bash
kasl template remove [OPTIONS] [NAME]
```

- `NAME`: Template to remove. Omit it in a terminal to pick from a list.
- `-y, --yes`: Remove without asking for confirmation.

Removing a template does not affect tasks already created from it.

## `kasl template search`

```bash
kasl template search <QUERY>
```

- `QUERY`: Text to match against template names and task names. Case-insensitive, partial matches included.

## Using a template with `task add`

```bash
kasl task add --template <NAME> [OPTIONS]
kasl task add --from-template
```

- `-t, --template <NAME>`: Create a task from a named template.
- `-l, --from-template`: Pick a template interactively (requires a terminal).

Any of `task add`'s own `--name`, `--comment`, `--completeness`, or `--tags` override the corresponding template value. In a terminal, fields left unset are prompted for with the template's value pre-filled for editing; outside a terminal (e.g. in a script) they are taken from the template as-is. Tags are never part of a template - `--tags` only ever comes from the command line.

```bash
# Override the task name, keep the template's comment and completeness
kasl task add --template code-review --name "Review PR #482"

# Non-interactive: unset fields come straight from the template
kasl task add --template daily-standup
```

## Sample Output

```
$ kasl template add --name standup --task-name "Daily standup" --comment "team sync" --completeness 0
✅ Template 'standup' created successfully.

$ kasl template list

Task Templates:

+---------------+---------------+-----------+--------------+
| TEMPLATE NAME | TASK NAME     | COMMENT   | COMPLETENESS |
+---------------+---------------+-----------+--------------+
| standup       | Daily standup | team sync | 0%           |
+---------------+---------------+-----------+--------------+
```

## Related commands

- [`task`](/reference/task/) - Create tasks, including from a template
- [`report`](/reference/report/) - View tasks created from templates
- [`export`](/reference/export/) - Export task data
