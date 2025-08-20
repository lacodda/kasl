# `template` Command

The `template` command provides comprehensive template management functionality for kasl, enabling users to create, edit, delete, and search reusable task templates. Templates streamline the creation of frequently used tasks by providing predefined values for name, comment, and completion status.

## Usage

```bash
kasl template [COMMAND] [OPTIONS]
```

## Commands

### `create` - Create a new task template

```bash
kasl template create [OPTIONS]
```

**Options:**
- `-n, --name <NAME>`: Unique name identifier for the template
  - Must be unique across all templates
  - Should be descriptive enough to easily identify the template's purpose
  - Used for referencing the template in task creation commands

**Examples:**
```bash
# Create template interactively
kasl template create

# Create template with name
kasl template create --name "daily-standup"
```

### `list` - List all available templates

```bash
kasl template list
```

Displays a formatted table of all existing templates with their names, task names, comments, and default completion values.

**Example:**
```bash
kasl template list
```

### `edit` - Edit an existing template

```bash
kasl template edit [NAME] [OPTIONS]
```

**Arguments:**
- `NAME`: Name of the template to edit (optional)
  - If not provided, an interactive selection interface will be presented

**Examples:**
```bash
# Edit template interactively
kasl template edit

# Edit specific template
kasl template edit "daily-standup"
```

### `delete` - Delete a template

```bash
kasl template delete [NAME] [OPTIONS]
```

**Arguments:**
- `NAME`: Name of the template to delete (optional)
  - If not provided, an interactive selection interface will be presented

**Options:**
- `--force`: Skip confirmation prompt

**Examples:**
```bash
# Delete template with confirmation
kasl template delete "old-template"

# Force delete without confirmation
kasl template delete "old-template" --force
```

### `search` - Search templates

```bash
kasl template search <QUERY>
```

**Arguments:**
- `QUERY`: Search query string
  - Searches both template names and task names for matches
  - Case-insensitive partial matching is supported

**Examples:**
```bash
# Search for templates containing "meeting"
kasl template search "meeting"

# Search for templates containing "daily"
kasl template search "daily"
```

## Template Features

### Template CRUD Operations
- **Create**: Define new templates with predefined task values
- **Read**: List and view existing templates
- **Update**: Modify template properties
- **Delete**: Remove templates from the system

### Search Functionality
Find templates by name or content:
- **Name Search**: Find templates by template name
- **Content Search**: Find templates by task name or comment
- **Partial Matching**: Case-insensitive search with partial matches

### Interactive Management
User-friendly interfaces for all operations:
- **Interactive Creation**: Guided template creation process
- **Interactive Selection**: Choose from available templates
- **Confirmation Prompts**: Prevent accidental deletions

### Integration
Seamless integration with task creation workflows:
```bash
# Use template when creating task
kasl task --from-template

# Use specific template
kasl task --template "daily-standup"
```

## Use Cases

### Daily Routines
```bash
# Create daily standup template
kasl template create --name "daily-standup"
# Template: Task name: "Daily standup", Comment: "Team sync meeting", Completeness: 0

# Create daily planning template
kasl template create --name "daily-planning"
# Template: Task name: "Plan day", Comment: "Review and plan daily tasks", Completeness: 0
```

### Meeting Templates
```bash
# Create client meeting template
kasl template create --name "client-meeting"
# Template: Task name: "Client meeting", Comment: "Discuss project requirements", Completeness: 0

# Create team meeting template
kasl template create --name "team-meeting"
# Template: Task name: "Team meeting", Comment: "Weekly team sync", Completeness: 0
```

### Development Tasks
```bash
# Create code review template
kasl template create --name "code-review"
# Template: Task name: "Code review", Comment: "Review pull request", Completeness: 0

# Create bug fix template
kasl template create --name "bug-fix"
# Template: Task name: "Fix bug", Comment: "Investigate and fix reported issue", Completeness: 0
```

### Administrative Tasks
```bash
# Create documentation template
kasl template create --name "documentation"
# Template: Task name: "Update documentation", Comment: "Update project documentation", Completeness: 0

# Create email template
kasl template create --name "email"
# Template: Task name: "Email correspondence", Comment: "Respond to emails", Completeness: 0
```

## Examples

### Complete Workflow

```bash
# 1. Create templates for common tasks
kasl template create --name "daily-standup"
kasl template create --name "code-review"
kasl template create --name "client-meeting"

# 2. List all templates
kasl template list

# 3. Use templates to create tasks
kasl task --from-template
# Select from available templates

# 4. Use specific template
kasl task --template "daily-standup"
```

### Template Management

```bash
# Create comprehensive template library
kasl template create --name "morning-routine"
kasl template create --name "afternoon-review"
kasl template create --name "end-of-day"
kasl template create --name "weekly-planning"

# Edit template properties
kasl template edit "morning-routine"

# Search for specific templates
kasl template search "meeting"

# Delete unused templates
kasl template delete "old-template"
```

### Interactive Usage

```bash
# Interactive template creation
kasl template create
# Prompts for template name, task name, comment, and completeness

# Interactive template selection
kasl template edit
# Shows list of available templates to choose from

# Interactive template deletion
kasl template delete
# Shows list and prompts for confirmation
```

## Sample Output

### Template List
```
+---------------+---------------+---------------------+-------------+
| TEMPLATE NAME | TASK NAME     | COMMENT             | COMPLETENESS|
+---------------+---------------+---------------------+-------------+
| daily-standup | Daily standup | Team sync meeting   | 0%          |
| code-review   | Code review   | Review PR           | 0%          |
| client-meeting| Client meeting| Discuss requirements| 0%          |
| bug-fix       | Fix bug       | Investigate and fix | 0%          |
| documentation | Update docs   | Update documentation| 0%          |
+---------------+---------------+---------------------+-------------+
```

### Template Creation
```
Creating new template...

Template name: daily-standup
Task name: Daily standup
Comment: Team sync meeting
Default completeness (0-100): 0

✅ Template 'daily-standup' created successfully!
```

### Template Search
```
Searching for templates containing "meeting"...

Results:
├── client-meeting: Client meeting - Discuss requirements
├── team-meeting: Team meeting - Weekly team sync
└── daily-standup: Daily standup - Team sync meeting

Found 3 matching templates.
```

### Template Editing
```
Editing template 'daily-standup'

Current properties:
├── Task name: Daily standup
├── Comment: Team sync meeting
└── Completeness: 0%

New task name (press Enter to keep current): Daily standup meeting
New comment (press Enter to keep current): Daily team standup meeting
New completeness (press Enter to keep current): 0

✅ Template updated successfully!
```

## Using Templates with Tasks

### Interactive Template Selection
```bash
# Create task using template selection
kasl task --from-template

# The command will show available templates:
# 1. daily-standup
# 2. code-review
# 3. client-meeting
# Select template: 1
```

### Direct Template Usage
```bash
# Use specific template
kasl task --template "daily-standup"

# Use template with additional options
kasl task --template "code-review" --tags "urgent,backend"
```

### Template with Customization
```bash
# Use template but override task name
kasl task --template "bug-fix" --name "Fix login bug"

# Use template but add custom comment
kasl task --template "client-meeting" --comment "Discuss new feature requirements"
```

## Best Practices

### Template Naming

1. **Use descriptive names**: "daily-standup" instead of "ds"
2. **Be consistent**: Use the same naming convention
3. **Keep it simple**: Avoid overly complex template names
4. **Use lowercase**: For consistency and easier typing

### Template Content

1. **Generic task names**: Allow for customization when used
2. **Clear comments**: Provide helpful default descriptions
3. **Appropriate completeness**: Usually 0% for new tasks
4. **Reusable structure**: Make templates flexible for different contexts

### Template Organization

1. **Group related templates**: Use consistent naming patterns
2. **Regular cleanup**: Delete unused templates
3. **Document template purposes**: Keep track of when to use each template
4. **Review and update**: Keep templates current with your workflow

### Template Usage

1. **Use templates frequently**: For any recurring task type
2. **Customize when needed**: Override template values when appropriate
3. **Combine with tags**: Use templates with tags for better organization
4. **Review effectiveness**: Update templates based on usage patterns

## Integration with Other Commands

The `template` command works with other kasl commands:

- **`task`**: Use templates to create tasks quickly
- **`tag`**: Combine templates with tags for better organization
- **`report`**: View tasks created from templates in reports
- **`export`**: Export template data for backup or sharing

## Troubleshooting

### Common Issues

**Template name already exists**
```bash
# Check existing templates
kasl template list

# Use different name or edit existing template
kasl template edit "existing-template"
```

**Template not found**
```bash
# List all templates to see available options
kasl template list

# Search for similar templates
kasl template search "partial-name"
```

**Template not working with task creation**
```bash
# Verify template exists
kasl template list

# Check template name spelling
kasl template search "template-name"
```

### Data Recovery

```bash
# Export templates before deletion
kasl export --format json

# Review template usage before deletion
kasl template list
```

## Related Commands

- **[`task`](./task.md)** - Create tasks using templates
- **[`tag`](./tag.md)** - Combine templates with tags
- **[`report`](./report.md)** - View tasks created from templates
- **[`export`](./export.md)** - Export template data
