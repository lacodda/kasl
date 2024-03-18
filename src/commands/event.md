
# `event` Command

The `event` command in `kasl` is used for managing and displaying events related to the application's operations. This command allows users to insert new events or display information about existing events, depending on the provided arguments.

## Usage

```plaintext
kasl event [OPTIONS] <EVENT_TYPE>
```

### Options

- `<EVENT_TYPE>`: Specifies the type of the event to insert. Defaults to `start`. The `EVENT_TYPE` can be either `start` or `end`, representing the beginning and the end of an event, respectively. This option is used when adding a new event and is not required when simply viewing events.

- `-s`, `--show`: Displays the events information. When this option is provided, the command will fetch and display information about events instead of inserting a new event.

### Examples

- Inserting a new `start` event:

  ```bash
  kasl event start
  ```

- Inserting a new `end` event:

  ```bash
  kasl event end
  ```

- Displaying information about events:

  ```bash
  kasl event --show
  ```

## Description

The `event` command operates in two modes: insert and display.

- **Insert Mode:** When called without the `--show` option, it inserts a new event of the specified type into the application's database. By default, it inserts a `start` event unless another event type is specified via the `--event_type` option. This mode is useful for tracking the start and end of events within the application.

- **Display Mode:** When the `--show` option is used, the command fetches events from the database, merges them based on certain criteria, calculates durations, and formats them for display. It then prints a summary of the working hours and details of each event. This mode is intended for reviewing the events and their durations over time.

### Implementation Notes

- The command uses the local system time (`chrono::Local`) to display the current date when showing events.

- It leverages the application's `Events`, `EventType`, `FormatEvents`, `MergeEvents`, and `View` modules for database operations, event handling, and output formatting.

- The command is designed to be robust, with error handling that ensures graceful failure in case of issues accessing the database or processing events.

## Error Handling

The `event` command returns an error if there are issues accessing the database, merging events, or any other operation-related failures. It ensures that errors are properly boxed and communicated back to the caller, facilitating debugging and resolution.
