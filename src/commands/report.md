
# `report` Command

The `report` command in `kasl` is designed to generate and optionally send a comprehensive report covering the day's events and tasks. This command aggregates data from events and tasks, formats it, and displays or sends it as specified by the user.

## Usage

```plaintext
kasl report [OPTIONS]
```

### Options

- `--send`: When this option is provided, the command will attempt to send the generated report. If not specified, the command will display the report for the current day without sending it.

### Examples

- Generating and displaying a report for today:

  ```bash
  kasl report
  ```

- Generating and sending today's report:

  ```bash
  kasl report --send
  ```

## Description

The `report` command operates in two main modes based on the provided options:

- **Display Mode:** By default, without the `--send` option, the command fetches today's events and tasks, merges and formats the events, and displays a report. This report includes a summary of the events and a list of tasks for the current day.

- **Send Mode:** When the `--send` option is used, the command performs the same data fetching and processing as in display mode. Additionally, it attempts to send the formatted report. If no tasks are found for the day, it notifies the user that no tasks are available. On successful submission, a confirmation message is shown, indicating that the report has been sent.

### Implementation Notes

- The command utilizes local system time (`chrono::Local`) for timestamping and report dating.

- It leverages multiple modules from the application, such as `Events`, `Tasks`, `Config`, and `Si` for database operations, configuration management, and server interaction.

- Data from events is serialized into JSON format for sending. The `Si` module is responsible for transmitting this data based on application configurations.

- Error handling is built into the command to manage and report issues with fetching data, reading configuration, or communicating with the server.

## Error Handling

The command is designed to handle various errors gracefully, including database access issues, configuration problems, and network communication errors. Errors are reported to the user with sufficient detail to identify and address the underlying problem.

---

This documentation provides users with a clear understanding of how to use the `report` command within the `kasl` utility, including its options, operation modes, and handling of different scenarios.
