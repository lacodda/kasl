# Architecture Decision Records

Technical decisions that shape kasl, in the order they were made. Format: Context / Decision / Consequences. A superseded ADR is never deleted - its status changes and it points to the successor.

| # | Title | Status |
| --- | --- | --- |
| [0001](0001-os-keyring.md) | Credentials live in the OS keyring | accepted |
| [0002](0002-sqlite-schema.md) | SQLite with versioned migrations | accepted |
| [0003](0003-activity-daemon.md) | Activity monitoring as a self-spawning daemon | accepted |
| [0004](0004-the-pulse.md) | The pulse: one sender, consent per connection, one row | accepted |
| [0005](0005-jira-moves-by-category.md) | Jira moves by status category, and says why an issue left | accepted |
| [0006](0006-inbox-rules-act-on-an-edge.md) | Inbox rules act on an edge, and a bad rule is skipped | accepted |
| [0007](0007-output-for-scripts-and-logs.md) | Output for scripts and logs - `--json`, `--plain`, one colour decision | accepted |
