---
title: "Features"
---

An overview of what kasl does and how its pieces fit together. Command-level detail lives in [Reference](/reference/watch/); this page is the map.

## Activity Monitoring

A background daemon (`kasl watch`) watches keyboard and mouse input and turns it into workdays, work intervals and pauses - nothing to start or stop by hand.

- **Workday start**: begins once continuous activity reaches `activity_threshold`.
- **Pause detection**: an inactivity gap of `pause_threshold` or more opens a pause; it closes and is kept if it lasted at least `min_pause_duration`.
- **Background operation**: runs as a daemon; `kasl watch --foreground` runs it in the current terminal with live output instead.

```json
{
  "monitor": {
    "min_pause_duration": 20,    // minutes - shortest break that gets recorded
    "pause_threshold": 60,       // seconds - inactivity before a pause opens
    "poll_interval": 500,        // milliseconds - activity check interval
    "activity_threshold": 30,    // seconds - activity required to start a workday
    "min_work_interval": 10      // minutes - shortest interval kept in reports
  }
}
```

See [Configuration](/concepts/configuration/) for the full reference.

## Recording Missed Absences

The monitor only sees keyboard and mouse, so an absence spent away from the machine leaves no trace. [`kasl pauses add`](/reference/pauses/) puts it on the record:

- **Explicit times**: you state when the absence began and how long it lasted - nothing is inferred.
- **`--keep`**: exempts an entry from the duration filter and from being merged into an adjacent detected pause.
- **Conflict prevention**: an entry overlapping a pause already on record is rejected rather than silently merged.

```bash
kasl pauses add --start 13:00 --minutes 60 --reason "lunch"
kasl pauses add --start 16:20 --minutes 10 --keep
```

## Productivity

```text
Available Work Time = Workday Length - Long Pauses
Net Work Time       = Available Work Time - Short Pauses
Productivity         = Net Work Time / Available Work Time * 100
```

Long pauses - detected absences at or above `min_pause_duration`, plus every manual pause - leave the denominator entirely; short pauses lower the numerator only. `kasl report --send` and `kasl sum --send` refuse to submit when productivity falls below `min_productivity_threshold`, and `kasl report` warns once enough of the expected workday (`min_workday_fraction_before_suggest`) has passed while productivity is still low.

```json
{
  "productivity": {
    "min_productivity_threshold": 75.0,
    "workday_hours": 8.0,
    "min_workday_fraction_before_suggest": 0.5
  }
}
```

## Task Management

Full task lifecycle, tags, and templates:

```bash
kasl task add --name "Review PR" --comment "Security review" --completeness 0 --tags "urgent,backend"
kasl task list --tag "urgent"
kasl task edit 1
kasl task remove --today

kasl template add --name "daily-standup"
kasl task add --template "daily-standup"

kasl tag add "urgent" --color "red"
```

Completeness is a 0-100 percentage: 0 means not started, 100 means done, anything between is in progress.

## Reporting

```bash
kasl report          # today
kasl report --last   # yesterday
kasl report --send   # submit to the configured reporting API

kasl sum             # this month's totals and daily breakdown
kasl sum --send      # submit the monthly summary
```

A report shows work intervals, pauses, tasks, and productivity for one day; `sum` aggregates the current month - daily hours, an average, and rest-day integration from the company calendar. Both filter out intervals shorter than `min_work_interval` before display and before submission, so a few minutes of stray activity around a break does not clutter the numbers.

## Data Export

```bash
kasl export report --format csv
kasl export tasks --format json
kasl export summary --format excel
kasl export all --format json    # report + tasks + summary
```

Formats: CSV, pretty-printed JSON, or Excel (one worksheet per export, with headers and autofit).

## GitLab and Jira

Both are configured through `kasl setup` and feed `kasl task find`, which imports today's GitLab commits and Jira issues resolved today as completed tasks.

- **GitLab**: authenticates with a personal access token (`read_user` + `read_repository` scope); discovers commits from the user's push events.
- **Jira**: authenticates with a session cookie obtained from login credentials; supports custom fields (`jira_inbox.custom_fields`) so extra Jira fields can be pulled into the inbox view and used for sorting (`sort_by_field`).

`kasl inbox` keeps a separate, persistent list of assigned open Jira issues - synced, ranked, and actionable independently of `task find`:

```bash
kasl inbox sync
kasl inbox list
kasl inbox take ISSUE-123
```

## SiServer / Reporting API

`kasl setup` configures the corporate reporting endpoint used by `kasl report --send` and `kasl sum --send`. Authentication differs by integration: SiServer credentials go through the OS keyring like Jira's; a plain reporting server (`server.auth_token` in `config.json`) is authenticated with a static token sent on every submission.

## System Integration

### Autostart

```bash
kasl autostart enable
kasl autostart status
kasl autostart disable
```

- **Windows**: Task Scheduler, falling back to a `HKCU\...\Run` registry entry if that fails.
- **macOS**: a LaunchAgent (`~/Library/LaunchAgents/com.lacodda.kasl.plist`).
- **Linux**: a systemd user unit.

### Background Monitoring

```bash
kasl watch             # start the daemon
kasl watch --foreground
kasl watch --stop
```

### Debug Logging

kasl keeps no log file; diagnostics go through `tracing` to stderr, off by default:

```bash
KASL_DEBUG=1 kasl watch --foreground   # shorthand for RUST_LOG=kasl=debug
RUST_LOG=kasl=trace kasl watch --foreground
```

See [Troubleshooting](/guides/troubleshooting/) for how to use this when something goes wrong.

## Credentials and Secrets

- Jira and SiServer passwords live in the OS keyring - Credential Manager on Windows, Keychain on macOS, Secret Service on Linux - under the service name `lacodda.kasl`. Nothing is encrypted with a key compiled into the binary; that scheme was removed in 1.0 (see [ADR 0001](https://github.com/lacodda/kasl/blob/main/docs/adr/0001-os-keyring.md)).
- GitLab's access token and the reporting server's `auth_token` are stored as plain text in `config.json`, because they are read by the unattended background daemon and have no interactive prompt to fall back to. Keep the data directory private.

## Cross-Platform Support

kasl builds and runs on Windows 10+, macOS, and Linux, with no runtime dependencies beyond the OS keyring backend. Official release binaries target:

- `x86_64-pc-windows-msvc`
- `x86_64-unknown-linux-gnu`
- `aarch64-apple-darwin` (Apple Silicon)

Other platforms and architectures build from source via `cargo install kasl-cli`.

## Related pages

- [Configuration](/concepts/configuration/) - every setting referenced above
- [Database](/concepts/database/) - how the SQLite schema is organized
- [API Integrations](/concepts/api-integrations/) - GitLab, Jira and SiServer in more depth
- [Troubleshooting](/guides/troubleshooting/) - when a feature above does not behave as described
