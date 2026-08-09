# `tag` Command

The `tag` command provides comprehensive tag management functionality, enabling users to create, organize, and utilize tags for better task categorization. Tags serve as a flexible labeling system that allows users to group and filter tasks by project, priority, type, or any custom criteria.

## Usage

```bash
kasl tag [COMMAND]
```

## Commands

### `add` - Add a new tag

```bash
kasl tag add <NAME> [OPTIONS]
```

**Arguments:**
- `NAME`: Unique name for the tag (required)

**Options:**
- `-c, --color <COLOR>`: Optional color for visual organization
  - Common color names: "red", "blue", "green", "yellow", "purple", "orange"
  - Hex color codes: "#FF0000", "#00FF00", etc.

**Examples:**
```bash
# Add a simple tag
kasl tag add "urgent"

# Add a tag with color
kasl tag add "backend" --color "blue"

# Add a tag with hex color
kasl tag add "frontend" --color "#FF6B6B"
```

### `list` - List all available tags

```bash
kasl tag list
```

Displays a formatted table of all existing tags with their names, colors, and creation dates.

**Example:**
```bash
kasl tag list
```

### `show` - Show a tag and the tasks that carry it

```bash
kasl tag show <TAG>
```

**Arguments:**
- `TAG`: Tag name or ID to show

**Example:**
```bash
kasl tag show "urgent"
```

### `edit` - Edit an existing tag

```bash
kasl tag edit <TAG>
```

**Arguments:**
- `TAG`: Tag name or ID to edit

Prompts interactively for the new name and color.

**Examples:**
```bash
# Edit tag interactively
kasl tag edit "urgent"

# Edit by ID
kasl tag edit 1
```

### `remove` - Remove a tag

```bash
kasl tag remove <TAG> [OPTIONS]
```

**Arguments:**
- `TAG`: Tag name or ID to remove

**Options:**
- `-y, --yes`: Remove without asking for confirmation

**Examples:**
```bash
# Remove tag with confirmation
kasl tag remove "old-tag"

# Remove without confirmation
kasl tag remove "old-tag" -y
```

## Tag Features

### Tag CRUD Operations
- **Add**: Define new tags with names and colors
- **Read**: List and view existing tags
- **Update**: Modify tag properties
- **Remove**: Remove tags and clean up associations

### Color Coding
Visual organization with customizable tag colors:
- **Named Colors**: "red", "blue", "green", "yellow", "purple", "orange"
- **Hex Colors**: "#FF0000", "#00FF00", "#0000FF"
- **Visual Impact**: Colors appear in task listings and reports

### Task Association
Link tags to tasks for categorization:
```bash
# Create task with tags
kasl task add --name "Fix bug" --tags "urgent,backend"

# Add tags to existing task
kasl task edit 1
```

### Filtering
Find tasks by tag assignments:
```bash
# Show tasks with specific tag
kasl task list --tag "urgent"
```

## Use Cases

### Project Organization
```bash
# Create project tags
kasl tag add "frontend" --color "blue"
kasl tag add "backend" --color "green"
kasl tag add "mobile" --color "purple"

# Assign to tasks
kasl task add --name "Update UI" --tags "frontend"
kasl task add --name "Fix API bug" --tags "backend"
```

### Priority Management
```bash
# Create priority tags
kasl tag add "urgent" --color "red"
kasl tag add "high" --color "orange"
kasl tag add "low" --color "gray"

# Filter by priority
kasl task list --tag "urgent"
```

### Task Type Categorization
```bash
# Create type tags
kasl tag add "bug" --color "red"
kasl tag add "feature" --color "green"
kasl tag add "documentation" --color "blue"
kasl tag add "meeting" --color "yellow"

# Organize tasks by type
kasl task add --name "Fix login bug" --tags "bug,urgent"
kasl task add --name "Add user profile" --tags "feature,frontend"
```

### Status Tracking
```bash
# Create status tags
kasl tag add "in-progress" --color "blue"
kasl tag add "blocked" --color "red"
kasl tag add "waiting-review" --color "yellow"
kasl tag add "completed" --color "green"

# Track task status
kasl task add --name "Code review" --tags "waiting-review,backend"
```

## Examples

### Complete Workflow

```bash
# 1. Create tags for your project
kasl tag add "frontend" --color "blue"
kasl tag add "backend" --color "green"
kasl tag add "urgent" --color "red"
kasl tag add "bug" --color "orange"

# 2. List all tags
kasl tag list

# 3. Create tasks with tags
kasl task add --name "Fix login bug" --tags "urgent,bug,frontend"
kasl task add --name "Add API endpoint" --tags "backend,feature"

# 4. Filter tasks by tags
kasl task list --tag "urgent"
kasl task list --tag "frontend"
```

### Tag Management

```bash
# Create a comprehensive tag system
kasl tag add "project-a" --color "blue"
kasl tag add "project-b" --color "green"
kasl tag add "urgent" --color "red"
kasl tag add "low-priority" --color "gray"
kasl tag add "meeting" --color "yellow"
kasl tag add "documentation" --color "purple"

# Edit tag properties
kasl tag edit "project-a"

# Remove unused tags
kasl tag remove "old-tag"
```

### Interactive Usage

```bash
# Interactive tag editing
kasl tag edit "existing-tag"
# Prompts for new name and color

# Interactive tag removal
kasl tag remove "unused-tag"
# Prompts for confirmation
```

### Scripting

`add` and `remove` take their required data as arguments and flags, so they can run unattended:

```bash
kasl tag add "urgent" --color "red"
kasl tag remove "old-tag" -y
```

## Sample Output

### Tag List
```
+----+----------+-------+
| ID | NAME     | COLOR |
+----+----------+-------+
| 1  | urgent   | red   |
| 2  | backend  | blue  |
| 3  | frontend | green |
| 4  | bug      | orange|
| 5  | meeting  | yellow|
+----+----------+-------+
```

### Tag Creation
```
✅ Tag 'urgent' created successfully!
Color: red
ID: 1
```

### Tag Editing
```
Editing tag 'urgent' (ID: 1)

Current properties:
├── Name: urgent
└── Color: red

New name (press Enter to keep current): critical
New color (press Enter to keep current): darkred

✅ Tag updated successfully!
```

## Auto-Creation

Tags are automatically created when assigned to tasks:

```bash
# This will create the "new-tag" tag if it doesn't exist
kasl task add --name "New task" --tags "new-tag,urgent"
```

## Best Practices

### Tag Naming

1. **Use descriptive names**: "frontend" instead of "fe"
2. **Be consistent**: Use the same naming convention
3. **Keep it simple**: Avoid overly complex tag names
4. **Use lowercase**: For consistency and easier typing

### Color Organization

1. **Use meaningful colors**: Red for urgent, green for completed
2. **Limit color palette**: Don't use too many different colors
3. **Consider accessibility**: Ensure colors are distinguishable
4. **Be consistent**: Use the same colors for similar concepts

### Tag Management

1. **Regular cleanup**: Remove unused tags
2. **Consolidate similar tags**: Merge duplicate concepts
3. **Document tag meanings**: Keep a reference of what each tag means
4. **Review usage**: Check which tags are most/least used

### Task Organization

1. **Use multiple tags**: Combine project, priority, and type tags
2. **Don't over-tag**: Avoid using too many tags per task
3. **Be consistent**: Use the same tags for similar tasks
4. **Review regularly**: Update tags as projects evolve

## Integration with Other Commands

The `tag` command works with other kasl commands:

- **`task`**: Create and manage tasks with tags
- **`report`**: View tasks organized by tags in reports
- **`export`**: Export task data with tag information
- **`sum`**: Include tag-based analysis in summaries

## Troubleshooting

### Common Issues

**Tag already exists**
```bash
# Check existing tags
kasl tag list

# Use different name or edit existing tag
kasl tag edit "existing-tag"
```

**Tag not found**
```bash
# List all tags to see available options
kasl tag list

# Check spelling and case sensitivity
kasl tag list | grep -i "tag-name"
```

**Color not supported**
```bash
# Use standard color names
kasl tag add "mytag" --color "red"

# Or use hex color codes
kasl tag add "mytag" --color "#FF0000"
```

### Data Recovery

```bash
# Export tasks with tags before deletion
kasl export tasks --format json

# Review tag usage before deletion
kasl tag show "tag-to-delete"
```

## Related Commands

- **[`task`](./task.md)** - Create and manage tasks with tags
- **[`report`](./report.md)** - View tasks organized by tags
- **[`export`](./export.md)** - Export task data with tag information
- **[`sum`](./sum.md)** - Include tag-based analysis in summaries
