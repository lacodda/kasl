---
title: "task"
---

The `task` command provides comprehensive task management functionality in kasl, including creating, displaying, updating, and organizing tasks. It supports various operations through subcommands and interactive interfaces, providing flexibility in how tasks are handled within the application.

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

**Options:**
- `-n, --name <NAME>`: Specifies the name of the task
  - Required for non-interactive creation
- `--comment <COMMENT>`: Adds a comment to the task
  - Optional additional information about the task
- `-c, --completeness <COMPLETENESS>`: Indicates the completeness of the task as a percentage (0-100)
  - 0% = not started, 100% = completed
- `--tags <TAGS>`: Assign tags to the task
  - Comma-separated list of tags for categorization
  - Tags are automatically created if they don't exist
- `-t, --template <TEMPLATE>`: Create from a named template
- `-l, --from-template`: Pick a template interactively

Without `--name` (or a full set of non-interactive options), `kasl task add` prompts interactively. Outside an interactive terminal it errors instead of hanging: "task name is required; pass --name outside an interactive terminal" - so it's safe to call from scripts as long as `--name` is provided.

### `list` - List tasks

```bash
kasl task list [OPTIONS]
```

**Options:**
- `-a, --all`: List tasks from every date, not just today
- `--tag <TAG>`: Only tasks carrying this tag

### `show` - Show tasks by id

```bash
kasl task show <ID>...
```

**Arguments:**
- `<ID>...`: One or more task ids to show

### `edit` - Edit a task by id, or several interactively

```bash
kasl task edit [ID]
```

**Arguments:**
- `[ID]`: Task id to edit; omit to pick several interactively

### `remove` - Remove tasks by id, or all of today's

```bash
kasl task remove [OPTIONS] [ID]...
```

**Arguments:**
- `[ID]...`: Task ids to remove

**Options:**
- `--today`: Remove every task recorded for today
- `-y, --yes`: Remove without asking for confirmation

### `find` - Find incomplete tasks and import from GitLab/Jira

```bash
kasl task find
```

- Shows a spinner while searching incomplete local tasks, today's Jira issues, and GitLab commits
- Presents a single consolidated MultiSelect (incomplete tasks first, then a separator, then Jira/GitLab)
- Filters out tasks already logged today, near-duplicate names, and names from `task_discovery.ignore_names`
- After import selection, optionally add items to the persistent ignore list
- Selected incomplete tasks prompt for an updated completeness percentage before insert

## Examples

### Basic Task Operations

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
```

### Interactive Operations

```bash
# Find and update incomplete tasks
kasl task find

# Show specific tasks
kasl task show 1

# Edit specific task
kasl task edit 1

# Edit several tasks interactively
kasl task edit

# Create task from template
kasl task add --from-template

# Use specific template
kasl task add --template "daily-standup"
```

### Task Management

```bash
# Remove specific tasks
kasl task remove 1 2 3

# Remove specific tasks without confirmation
kasl task remove 1 2 3 -y

# Remove all today's tasks
kasl task remove --today

# Remove all today's tasks without confirmation
kasl task remove --today -y
```

## Use Cases

### Daily Task Management

```bash
# Create today's tasks
kasl task add --name "Daily standup" --template "daily-standup"
kasl task add --name "Code review" --tags "urgent,backend"
kasl task add --name "Team meeting" --tags "meeting"

# Review and update progress
kasl task list
kasl task find

# Complete finished tasks
kasl task edit 1
```

### Project Organization

```bash
# Create project-specific tasks
kasl task add --name "Frontend bug fix" --tags "frontend,bug,urgent"
kasl task add --name "API documentation" --tags "backend,documentation"

# Filter by project
kasl task list --tag "frontend"
kasl task list --tag "backend"
```

### Template Usage

```bash
# Create templates for common tasks
kasl template add --name "bug-fix"
kasl template add --name "meeting"

# Use templates to create tasks
kasl task add --template "bug-fix" --name "Fix login issue"
kasl task add --template "meeting" --name "Client call"
```

### Scripting

Because `add`, `remove`, and their siblings are dedicated subcommands with their own flags, they can be called from scripts without triggering an interactive prompt as long as the required arguments are supplied:

```bash
# Non-interactive creation
kasl task add --name "Nightly build check" --completeness 0

# Non-interactive cleanup
kasl task remove --today -y
```

## Sample Output

### Task List
```
+---+----+----------+------------------+------------------+-------------+------------------+
| # | ID | TASK ID | NAME             | COMMENT          | COMPLETENESS| TAGS             |
+---+----+----------+------------------+------------------+-------------+------------------+
| 1 | 1  | 0       | Daily standup    | Team sync        | 100%        | meeting          |
| 2 | 2  | 0       | Code review      | Review PR #123   | 75%         | urgent           |
| 3 | 3  | 0       | Bug fix          | Fix login issue  | 0%          | bug, urgent      |
| 4 | 4  | 0       | Documentation    | Update API docs  | 25%         | docs             |
+---+----+----------+------------------+------------------+-------------+------------------+
```

### Interactive Task Selection
```
Select task to edit:
1. Daily standup (100%)
2. Code review (75%)
3. Bug fix (0%)
4. Documentation (25%)

Enter task number: 2

Editing task: Code review
Current completeness: 75%

New completeness (0-100): 100
New comment (press Enter to keep current): Review completed

✅ Task updated successfully!
```

## Integration with Other Commands

The `task` command works with other kasl commands:

- **`tag`**: Create and manage tags for task categorization
- **`template`**: Use templates for quick task creation
- **`report`**: View tasks in daily and monthly reports
- **`export`**: Export task data for external analysis

## Best Practices

### Task Organization

1. **Use descriptive names**: Clear, specific task names
2. **Add helpful comments**: Detailed descriptions for complex tasks
3. **Use tags consistently**: Establish tag conventions for your projects
4. **Update progress regularly**: Keep task completeness current

### Workflow Integration

1. **Create tasks at the start**: Plan your day with task creation
2. **Use templates**: Save time with reusable task templates
3. **Review regularly**: Check task status throughout the day
4. **Complete tasks promptly**: Mark tasks as done when finished

### Data Management

1. **Regular cleanup**: Remove completed tasks periodically
2. **Use filters**: Leverage tag and date filters for organization
3. **Backup data**: Export tasks before major cleanup operations
4. **Monitor patterns**: Review task completion patterns for insights

## Related Commands

- **[`tag`](/reference/tag/)** - Manage tags for task categorization
- **[`template`](/reference/template/)** - Create and use task templates
- **[`report`](/reference/report/)** - View tasks in work reports
- **[`export`](/reference/export/)** - Export task data for analysis
