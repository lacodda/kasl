
# `task` Command

The `task` command in `kasl` facilitates task management, including creating, displaying, and updating tasks. It supports various operations through command-line options, providing flexibility in how tasks are handled within the application.

## Usage

```plaintext
kasl task [OPTIONS]
```

### Options

- `-n`, `--name <NAME>`: Specifies the name of the task. This option is used for creating a new task or updating an existing one.

- `-c`, `--comment <COMMENT>`: Adds a comment to the task. This is optional and can be used for providing additional information about the task.

- `-p`, `--completeness <COMPLETENESS>`: Indicates the completeness of the task as a percentage (0-100). This can be used to update the task's progress.

- `-s`, `--show`: Displays tasks based on the specified filter. Without additional filtering options, it defaults to showing today's tasks.

- `-a`, `--all`: When used with `--show`, displays all tasks, overriding the default filter of today's tasks.

- `-i`, `--id <ID>`: Specifies one or more task IDs. When used with `--show`, filters the displayed tasks to those with the given IDs.

- `-f`, `--find`: Finds and allows the user to update incomplete tasks. This option triggers a user interface for selecting incomplete tasks and updating their completeness.

### Examples

- Creating a new task:

  ```bash
  kasl task --name "New Task" --comment "This is a test task" --completeness 50
  ```

- Displaying today's tasks:

  ```bash
  kasl task --show
  ```

- Displaying all tasks:

  ```bash
  kasl task --show --all
  ```

- Finding and updating incomplete tasks:

  ```bash
  kasl task --find
  ```

## Description

The `task` command allows for comprehensive task management. It supports creating new tasks, displaying tasks with various filters, and updating task completeness. The command integrates user inputs and selections for a smooth task management experience.

### Implementation Notes

- Uses `clap` for command-line argument parsing and `dialoguer` for interactive prompts and selections.

- Leverages the `Tasks` module for database operations, including fetching, creating, and updating tasks.

- Task filtering is versatile, supporting filters like `Today`, `All`, and `ByIds`, making it easy to display tasks as needed.

- The `View` module is used for formatting and displaying tasks, ensuring a consistent user experience.

## Error Handling

The command includes comprehensive error handling to address potential issues with database operations or user input. It provides clear feedback to the user in case of errors, facilitating troubleshooting and correction.

---

This documentation outlines the `task` command's functionality within the `kasl` utility, including options, usage examples, and details on its operation and error handling.
