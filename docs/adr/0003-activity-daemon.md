# ADR 0003: Activity monitoring as a self-spawning daemon

- Status: accepted
- Date: 2026-08-08

## Context

Detecting work sessions and pauses requires watching keyboard and mouse input continuously, independent of whatever `kasl` subcommand the user happens to run next. That watcher has to survive after the invoking terminal is closed, must not duplicate itself if started twice, and needs a way to be told to stop from a later, unrelated invocation of the CLI.

## Decision

`kasl watch` spawns the same executable as a detached background process rather than depending on a platform service manager. `daemon::spawn()` (`src/libs/daemon.rs`) resolves `std::env::current_exe()` and re-executes it with a single argument, `--daemon-run` - on Unix inside a new session (`setsid` in `pre_exec`), on Windows with `CREATE_NO_WINDOW`. That flag is not a clap subcommand: `run()` in `src/lib.rs` inspects raw `std::env::args()` and checks `args[1] == "--daemon-run"` *before* `commands::Cli::menu()` (clap parsing) ever runs. If matched, control goes straight to `commands::watch::run_as_daemon()`; otherwise the normal CLI dispatch happens. `watch --foreground` runs the same monitor loop (`run_monitor()` in `watch.rs`) synchronously in the current terminal instead, for debugging - no spawn, no PID file, no `--daemon-run`.

Single-instance control is a PID file (`kasl-watch.pid`, in the same per-user data directory as the database). `spawn()` checks for an existing PID file, tries to stop that process, and only then writes the new PID after a successful `Command::spawn()`. The running daemon deletes its own PID file on clean exit (`run_with_signal_handling`, after the monitor loop and the Jira-inbox poller both finish). Because of that self-deletion, `watch --stop` (`daemon::stop_internal`) treats every file operation as racing a daemon that may vanish mid-check: `read_to_string` returning `NotFound` is treated as "already stopped," not an error, and the PID file is removed again afterward regardless of whether the kill succeeded, so a stale file can never block a future `spawn()`.

Input activity is detected via the `rdev` crate: `Monitor::new` spawns a dedicated OS thread running `rdev::listen`, watching `KeyPress`/`KeyRelease`/`ButtonPress`/`ButtonRelease`/`MouseMove`/`Wheel`, and updates a shared `Arc<Mutex<Instant>>` timestamp on every event. The async monitor loop (`Monitor::run`) polls that timestamp against `poll_interval` to decide activity vs. inactivity, and drives a two-state machine (`Active`/`InPause`) that calls `Pauses::insert_start_with_time` / `Pauses::insert_end` and `Workdays::insert_start` directly against the SQLite connection each transition.

The daemon has no controlling terminal, and the `--daemon-run` process also runs the Jira-inbox poller (`libs::jira_inbox::run_poller`) as a sibling task, which needs the same Jira credential the interactive commands do. `Secret::get_or_prompt` would block on `dialoguer::Password::interact()` with nowhere to read from, so the daemon path (`jira_inbox::sync_noninteractive`) calls `Secret::try_get_cached()` instead, which only reads the OS keyring (and migrates a legacy encrypted file if present) and returns `None` rather than prompting when nothing is stored.

## Consequences

- `--daemon-run` is an internal implementation detail, not public API: it is undocumented in `--help`, bypasses clap entirely, and any change to argument parsing must keep the raw pre-clap check in `lib.rs` in sync with the literal string `daemon::spawn()` passes.
- Process lifecycle is PID-file based rather than OS-service based (no systemd unit, no Windows service, no launchd agent), so the daemon does not survive user logout/reboot on its own and must be restarted by whatever mechanism the user chooses (this ADR does not cover that).
- `stop_internal`'s "missing file means already stopped" logic is required correctness, not a convenience: without it, a `watch --stop` racing the daemon's own cleanup would report a spurious error on every clean shutdown.
- Any credential the daemon needs at runtime must have a `try_get_cached`-style non-interactive path; a feature that only offers `get_or_prompt` will silently fail (return an error, not hang) whenever it runs unattended.
- `is_process_running`/`kill_process` are implemented per-platform (WinAPI `OpenProcess`/`TerminateProcess` vs. shelling out to `ps`/`kill`), so process-management bugs can be platform-specific and must be checked on both.
