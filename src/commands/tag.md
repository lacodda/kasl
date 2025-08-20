# `tag` Command

The `tag` command provides comprehensive tag management functionality, enabling users to create, organize, and utilize tags for better task categorization. Tags serve as a flexible labeling system that allows users to group and filter tasks by project, priority, type, or any custom criteria.

## Usage

```bash
kasl tag [COMMAND] [OPTIONS]
```

## Commands

### `create` - Create a new tag

```bash
kasl tag create <NAME> [OPTIONS]
```

**Arguments:**
- `NAME`: Unique name for the tag (required)

**Options:**
- `-c, --color <COLOR>`: Optional color for visual organization
  - Common color names: "red", "blue", "green", "yellow", "purple", "orange"
  - Hex color codes: "#FF0000", "#00FF00", etc.

**Examples:**
```bash
# Create a simple tag
kasl tag create "urgent"

# Create a tag with color
kasl tag create "backend" --color "blue"

# Create a tag with hex color
kasl tag create "frontend" --color "#FF6B6B"
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

### `edit` - Edit an existing tag

```bash
kasl tag edit <TAG> [OPTIONS]
```

**Arguments:**
- `TAG`: Tag name or ID to edit

**Options:**
- `-n, --name <NAME>`: New name for the tag
- `-c, --color <COLOR>`: New color for the tag

**Examples:**
```bash
# Edit tag interactively
kasl tag edit "urgent"

# Edit tag with new name
kasl tag edit "urgent" --name "critical"

# Edit tag with new color
kasl tag edit "backend" --color "darkblue"
```

### `delete` - Delete a tag

```bash
kasl tag delete <TAG> [OPTIONS]
```

**Arguments:**
- `TAG`: Tag name or ID to delete

**Options:**
- `--force`: Skip confirmation prompt

**Examples:**
```bash
# Delete tag with confirmation
kasl tag delete "old-tag"

# Force delete without confirmation
kasl tag delete "old-tag" --force
```

## Tag Features

### Tag CRUD Operations
- **Create**: Define new tags with names and colors
- **Read**: List and view existing tags
- **Update**: Modify tag properties
- **Delete**: Remove tags and clean up associations

### Color Coding
Visual organization with customizable tag colors:
- **Named Colors**: "red", "blue", "green", "yellow", "purple", "orange"
- **Hex Colors**: "#FF0000", "#00FF00", "#0000FF"
- **Visual Impact**: Colors appear in task listings and reports

### Task Association
Link tags to tasks for categorization:
```bash
# Create task with tags
kasl task --name "Fix bug" --tags "urgent,backend"

# Add tags to existing task
kasl task --edit 1 --tags "urgent,backend"
```

### Filtering
Find tasks by tag assignments:
```bash
# Show tasks with specific tag
kasl task --show --tag "urgent"

# Show tasks with multiple tags
kasl task --show --tag "backend,urgent"
```

## Use Cases

### Project Organization
```bash
# Create project tags
kasl tag create "frontend" --color "blue"
kasl tag create "backend" --color "green"
kasl tag create "mobile" --color "purple"

# Assign to tasks
kasl task --name "Update UI" --tags "frontend"
kasl task --name "Fix API bug" --tags "backend"
```

### Priority Management
```bash
# Create priority tags
kasl tag create "urgent" --color "red"
kasl tag create "high" --color "orange"
kasl tag create "low" --color "gray"

# Filter by priority
kasl task --show --tag "urgent"
```

### Task Type Categorization
```bash
# Create type tags
kasl tag create "bug" --color "red"
kasl tag create "feature" --color "green"
kasl tag create "documentation" --color "blue"
kasl tag create "meeting" --color "yellow"

# Organize tasks by type
kasl task --name "Fix login bug" --tags "bug,urgent"
kasl task --name "Add user profile" --tags "feature,frontend"
```

### Status Tracking
```bash
# Create status tags
kasl tag create "in-progress" --color "blue"
kasl tag create "blocked" --color "red"
kasl tag create "waiting-review" --color "yellow"
kasl tag create "completed" --color "green"

# Track task status
kasl task --name "Code review" --tags "waiting-review,backend"
```

## Examples

### Complete Workflow

```bash
# 1. Create tags for your project
kasl tag create "frontend" --color "blue"
kasl tag create "backend" --color "green"
kasl tag create "urgent" --color "red"
kasl tag create "bug" --color "orange"

# 2. List all tags
kasl tag list

# 3. Create tasks with tags
kasl task --name "Fix login bug" --tags "urgent,bug,frontend"
kasl task --name "Add API endpoint" --tags "backend,feature"

# 4. Filter tasks by tags
kasl task --show --tag "urgent"
kasl task --show --tag "frontend"
```

### Tag Management

```bash
# Create a comprehensive tag system
kasl tag create "project-a" --color "blue"
kasl tag create "project-b" --color "green"
kasl tag create "urgent" --color "red"
kasl tag create "low-priority" --color "gray"
kasl tag create "meeting" --color "yellow"
kasl tag create "documentation" --color "purple"

# Edit tag properties
kasl tag edit "project-a" --name "main-project" --color "darkblue"

# Delete unused tags
kasl tag delete "old-tag"
```

### Interactive Usage

```bash
# Interactive tag creation
kasl tag create "new-tag"
# Prompts for color if not specified

# Interactive tag editing
kasl tag edit "existing-tag"
# Prompts for new name and color

# Interactive tag deletion
kasl tag delete "unused-tag"
# Prompts for confirmation
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
kasl task --name "New task" --tags "new-tag,urgent"
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

1. **Regular cleanup**: Delete unused tags
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
kasl tag create "mytag" --color "red"

# Or use hex color codes
kasl tag create "mytag" --color "#FF0000"
```

### Data Recovery

```bash
# Export tasks with tags before deletion
kasl export tasks --format json

# Review tag usage before deletion
kasl task --show --tag "tag-to-delete"
```

## Related Commands

- **[`task`](./task.md)** - Create and manage tasks with tags
- **[`report`](./report.md)** - View tasks organized by tags
- **[`export`](./export.md)** - Export task data with tag information
- **[`sum`](./sum.md)** - Include tag-based analysis in summaries
