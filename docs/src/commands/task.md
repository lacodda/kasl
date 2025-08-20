
# `task` Command

The `task` command provides comprehensive task management functionality in kasl, including creating, displaying, updating, and organizing tasks. It supports various operations through command-line options and interactive interfaces, providing flexibility in how tasks are handled within the application.

## Usage

```bash
kasl task [OPTIONS]
```

## Options

### Task Creation and Editing

- `-n, --name <NAME>`: Specifies the name of the task
  - Used for creating a new task or updating an existing one
  - Required for task creation

- `-c, --comment <COMMENT>`: Adds a comment to the task
  - Optional additional information about the task
  - Useful for detailed task descriptions

- `-p, --completeness <COMPLETENESS>`: Indicates the completeness of the task as a percentage (0-100)
  - Used to update the task's progress
  - 0% = not started, 100% = completed

- `--tags <TAGS>`: Assign tags to the task
  - Comma-separated list of tags for categorization
  - Tags are automatically created if they don't exist

### Task Display and Filtering

- `-s, --show`: Displays tasks based on the specified filter
  - Without additional filtering options, defaults to showing today's tasks

- `-a, --all`: When used with `--show`, displays all tasks
  - Overrides the default filter of today's tasks

- `-i, --id <ID>`: Specifies one or more task IDs
  - When used with `--show`, filters the displayed tasks to those with the given IDs

- `--tag <TAG>`: Filter tasks by tag
  - Shows only tasks with the specified tag
  - Can be combined with other filters

### Interactive Operations

- `-f, --find`: Finds and allows the user to update incomplete tasks
  - Triggers a user interface for selecting incomplete tasks and updating their completeness

- `--edit <ID>`: Edit a specific task by ID
  - Opens interactive editor for the specified task

- `--edit-interactive`: Edit tasks interactively
  - Shows list of tasks to choose from for editing

- `--from-template`: Create task from template
  - Shows available templates to choose from

- `--template <TEMPLATE>`: Use specific template for task creation
  - Creates task using the specified template

### Task Management

- `--delete <IDS>`: Delete tasks by ID
  - Accepts multiple task IDs separated by spaces

- `--delete-today`: Delete all tasks for today
  - Removes all tasks created today

- `--delete-all`: Delete all tasks
  - Removes all tasks from the database (use with caution)

## Examples

### Basic Task Operations

```bash
# Create a new task
kasl task --name "New Task" --comment "This is a test task" --completeness 50

# Create task with tags
kasl task --name "Fix bug" --tags "urgent,backend" --completeness 0

# Display today's tasks
kasl task --show

# Display all tasks
kasl task --show --all

# Display tasks with specific tag
kasl task --show --tag "urgent"
```

### Interactive Operations

```bash
# Find and update incomplete tasks
kasl task --find

# Edit specific task
kasl task --edit 1

# Edit tasks interactively
kasl task --edit-interactive

# Create task from template
kasl task --from-template

# Use specific template
kasl task --template "daily-standup"
```

### Task Management

```bash
# Update task completeness
kasl task --edit 1 --completeness 75

# Delete specific tasks
kasl task --delete 1 2 3

# Delete all today's tasks
kasl task --delete-today

# Delete all tasks (use with caution)
kasl task --delete-all
```

## Use Cases

### Daily Task Management

```bash
# Create today's tasks
kasl task --name "Daily standup" --template "daily-standup"
kasl task --name "Code review" --tags "urgent,backend"
kasl task --name "Team meeting" --tags "meeting"

# Review and update progress
kasl task --show
kasl task --find

# Complete finished tasks
kasl task --edit 1 --completeness 100
```

### Project Organization

```bash
# Create project-specific tasks
kasl task --name "Frontend bug fix" --tags "frontend,bug,urgent"
kasl task --name "API documentation" --tags "backend,documentation"

# Filter by project
kasl task --show --tag "frontend"
kasl task --show --tag "backend"
```

### Template Usage

```bash
# Create templates for common tasks
kasl template create --name "bug-fix"
kasl template create --name "meeting"

# Use templates to create tasks
kasl task --template "bug-fix" --name "Fix login issue"
kasl task --template "meeting" --name "Client call"
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

1. **Regular cleanup**: Delete completed tasks periodically
2. **Use filters**: Leverage tag and date filters for organization
3. **Backup data**: Export tasks before major cleanup operations
4. **Monitor patterns**: Review task completion patterns for insights

## Related Commands

- **[`tag`](./tag.md)** - Manage tags for task categorization
- **[`template`](./template.md)** - Create and use task templates
- **[`report`](./report.md)** - View tasks in work reports
- **[`export`](./export.md)** - Export task data for analysis
