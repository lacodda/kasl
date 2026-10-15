# ADR 0007: Output for scripts and logs - `--json`, `--plain`, one colour decision

- Status: accepted
- Date: 2026-10-15

## Context

By 1.16 kasl printed for a person only: framed tables cut to the terminal's width, emoji marks on every message, warnings on stdout between table rows. Three readers were left out:

1. **Programs.** A status bar, a script, rigger's task view and (from 1.18) an MCP server need the data, not a table to scrape. rigger already called `kasl inbox list --json` and got clap's "unexpected argument".
2. **Logs and pipes.** In CI and behind `grep`, a table cut at 100 columns loses facts, emoji marks are noise, and a warning printed among the rows becomes a row.
3. **Colour.** kasl does not paint much, but the crates it uses each decided on their own: `console` (the spinner, the prompts) honours `NO_COLOR` only on Unix and lets `CLICOLOR_FORCE` beat it; `tracing-subscriber` honours `NO_COLOR` but paints into a pipe - and wrote the debug log to stdout; clap colours help and usage errors with anstream's rules.

## Decision

**`--json` on the commands that print data, flattened, not global.** One `Format` struct (`--json`) is flattened into each reading command - `report`, `sum`, `pauses list`, `task list/show`, `tag list/show`, `template list/show/search`, `inbox list/show/rules`, `server status/queue/pulse/manifest`, `autostart status`. A command with no data to print refuses the flag as an unknown argument at parse time, instead of answering a script in prose. Whether a run asked for JSON is read off clap's matches wherever the flag sits in the subcommand chain. The two bare forms that carry list flags at the top (`kasl inbox --json`, `kasl pauses --json`) refuse when followed by a subcommand that does not list.

**Stdout is the document.** In JSON mode stdout carries exactly one JSON value and nothing else: every message goes to stderr, the debug log goes to stderr, the update check is skipped.

**One contract for every document** (`src/libs/json.rs`): the top level is an object; every key is always present, with `null` for a value that does not exist and `[]` for an empty list; moments are the machine's local time as `YYYY-MM-DDTHH:MM:SS`, whole seconds, no offset - how kasl stores them and what readers on the same machine compare against; dates are `YYYY-MM-DD`; durations are whole seconds in `*_seconds` fields; percentages are 0-100, one decimal. Shapes are built from the domain types in that one module, not derived on them, so the database can change shape without the contract noticing. Commands that compute (the day, the month) read once into a struct that both the table and the document render.

**`--plain` is global and means "the same lines without decoration".** No colour, no emoji (a warning says `warning:`, an error `error:`), no blank padding, tables as tab-separated lines with a header row, cells never cut or wrapped, no spinner. `--json` implies it for what still reaches stderr. The format changes only by flag, never because stdout is not a terminal.

**Diagnostics on stderr, always.** Warnings and errors go to stderr in every mode, so they never land between the rows a pipe is reading.

**One colour decision, handed to every painter.** `libs::output::colour` decides per stream in anstream's order: `--plain`, then a non-empty `NO_COLOR`, then `CLICOLOR_FORCE` other than `0`, then `CLICOLOR=0`, then whether the stream is a terminal. The result goes to `console` (`set_colors_enabled[_stderr]`) and to the tracing subscriber (`with_ansi`, writer stderr); clap gets `ColorChoice::Never` when `--plain` is among the arguments, since it prints before kasl has parsed anything, and decides the rest itself in the same order.

**A failed run prints its error once, marked for its reader.** `run()` prints the error chain on one line with `❌` or `error:`, and returns exit code 1. Asking for one named thing that is not there (`task show ID`, `tag show`, `template show`, `inbox show`) is such a failure in every mode.

**A reader that hangs up early is not a crash.** A panic hook turns the standard library's "failed printing to stdout" on a closed pipe into a quiet exit 0, as git and coreutils do.

## Consequences

- `kasl inbox list --all --snoozed --json` answers rigger from the local table, without reaching Jira.
- The MCP server (1.18) calls the same read functions and returns the same shapes; a field added for one reader is there for all.
- Adding `--json` to an action command later (`task add` returning the new id) is additive: nobody can have been relying on it being ignored.
- Warnings moved from stdout to stderr for people too. `kasl inbox | less` no longer shows the skipped-rule warning inside the pager; it shows above it.
- The `console` hand-off (spinner, prompts) is not covered by a test: it is observable only on a real terminal.
