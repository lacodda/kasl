---
title: "autostart"
---

The `autostart` command controls whether kasl starts monitoring automatically when the system boots or the user logs in. The mechanism is per-user, not system-wide: the daemon tracks one person's activity, so it has no business running as root or before login.

## Usage

```bash
kasl autostart [COMMAND]
```

| Subcommand | Purpose |
| --- | --- |
| `enable` | Enable autostart |
| `disable` | Disable autostart |
| `status` | Show current autostart status |

## `kasl autostart enable`

```bash
kasl autostart enable
```

**Windows**: tries the Windows Task Scheduler first (requires administrator privileges); if that is denied, falls back automatically to a per-user Registry Run key.

**macOS**: a LaunchAgent plist written to `~/Library/LaunchAgents`.

**Linux**: a systemd user unit, enabled for the current user. The unit file is
written even where systemd is absent, so it takes effect on the next login.

## `kasl autostart disable`

```bash
kasl autostart disable
```

Removes the autostart configuration. On Windows this attempts to remove both the Task Scheduler entry and the Registry Run key, regardless of which one is actually present.

## `kasl autostart status`

```bash
kasl autostart status
```

Reports whether autostart is currently enabled or disabled. The check only reports one of these two states - it does not report which mechanism (Task Scheduler vs. Registry, or LaunchAgent vs. systemd) is in use.

## Sample Output

```
ℹ️ Autostart has been enabled. Kasl will start automatically on system boot.
```

```
ℹ️ Autostart has been enabled for current user. Kasl will start when you log in.
```

```
ℹ️ Autostart is currently: enabled
```

`status` prints either `enabled` or `disabled` - nothing else.

## Examples

```bash
# Enable autostart
kasl autostart enable

# Check current status
kasl autostart status

# Disable autostart
kasl autostart disable
```

## Related commands

- [`watch`](/reference/watch/) - the monitoring command that autostart enables
- [`setup`](/reference/setup/) - configure kasl before setting up autostart
- [`self-update`](/reference/self-update/) - update kasl while preserving autostart settings
