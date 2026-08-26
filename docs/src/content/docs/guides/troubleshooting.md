---
title: "Troubleshooting"
sidebar:
  order: 7
---

Common problems and how to check what is actually happening, rather than guessing.

## Enable debug logging first

kasl has no log file. Diagnostic output goes through `tracing` to stderr, and is silent unless `RUST_LOG` or `KASL_DEBUG` is set:

```bash
RUST_LOG=kasl=debug kasl watch --foreground
```

`RUST_LOG=kasl=trace` gives more detail; `KASL_DEBUG=1` is a shorthand equivalent to `RUST_LOG=kasl=debug` for kasl's own messages. Reproduce the problem with one of these set before doing anything else below - most of the following sections just point you back here.

## Where kasl keeps its files

Everything lives under one data directory:

| Platform | Path |
| --- | --- |
| Windows | `%LOCALAPPDATA%\lacodda\kasl` |
| macOS | `~/Library/Application Support/lacodda/kasl` |
| Linux | `~/.local/share/lacodda/kasl` |

Inside it: `kasl.db` (SQLite database), `config.json` (settings and API credentials), and session cookie files (`.jira_session_id`, `.si_session_id`) written after a successful login. GitLab authenticates with a personal access token on every request, so it has no session file.

## Activity monitoring

### `kasl watch` does not detect a workday

1. Run it in the foreground to see activity as it happens:
   ```bash
   RUST_LOG=kasl=debug kasl watch --foreground
   ```
2. Check for an already-running daemon before starting another:
   ```bash
   # Linux/macOS
   ps aux | grep kasl
   # Windows
   tasklist | findstr kasl
   ```
   Stop it cleanly with `kasl watch --stop` before restarting.
3. If the workday starts too late or too early, adjust `activity_threshold` (seconds of continuous activity required to start a workday) via `kasl setup`. See [Configuration](/concepts/configuration/).

### Pauses are not detected, or are detected too eagerly

`pause_threshold` (seconds of inactivity before a pause starts) and `min_pause_duration` (minutes before a pause is kept) control this - reconfigure with `kasl setup`. An absence spent away from the machine (meeting in another room, laptop closed) leaves no trace at all; record it by hand with [`kasl pauses add`](/reference/pauses/).

## Database issues

### "database is locked"

Only one process should hold the database at a time. Stop any running daemon first:

```bash
kasl watch --stop
```

Then check that nothing else is writing to `kasl.db` (a second `watch` instance, a report running concurrently). If the file itself looks suspect:

```bash
sqlite3 "<data dir>/kasl.db" "PRAGMA integrity_check;"
```

## Configuration issues

### `config.json` missing or invalid

```bash
kasl setup            # recreate interactively
kasl setup --delete   # wipe it and start over
```

`config.json` is plain JSON; a `jq . config.json` or `python -m json.tool config.json` will point at a syntax error if the file was hand-edited.

### Credentials keep getting asked for again

Jira and SiServer passwords are stored in the OS keyring (Credential Manager / Keychain / Secret Service), not in `config.json`. On Linux, a background `kasl watch` daemon needs a running Secret Service provider (e.g. `gnome-keyring` or `kwallet`) to read the stored password without a terminal to prompt at; on a headless box without one, credential lookups fail silently and Jira/SiServer features are skipped. GitLab's access token and the reporting server's `auth_token` are stored directly in `config.json`, not the keyring - keep that file's permissions private.

## API integration issues

### Jira or SiServer authentication fails repeatedly

Delete the stale session file and let the next request log in again:

```bash
rm "<data dir>/.jira_session_id"
rm "<data dir>/.si_session_id"
```

If that does not help, re-run `kasl setup` to re-enter credentials - they may have expired or changed on the server side.

### GitLab requests fail

GitLab uses a personal access token (`gitlab.access_token` in `config.json`), not a session. Confirm the token is valid and has `read_user` + `read_repository` scope, and that `gitlab.api_url` points at the instance root (no `/api/v4` suffix).

### Report submission is refused with a productivity warning

`kasl report --send` refuses to submit when the day's productivity is below `min_productivity_threshold` (the monthly `kasl sum --send` has no such check) (see [Configuration](/concepts/configuration/)). This is intentional - it is the safeguard the removed `kasl breaks` command used to defeat. Record any genuine absence the monitor missed with [`kasl pauses add`](/reference/pauses/) before retrying; that is the only supported way to raise the number.

## Platform-specific issues

### Windows: autostart does not run at login

`kasl autostart enable` tries Task Scheduler first, then falls back to a `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` registry entry if that fails. Check both:

```cmd
schtasks /query /tn KaslAutostart
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v Kasl
```

`kasl autostart status` reports which one (if either) is active.

### macOS: no activity detected

Grant Accessibility and Input Monitoring permissions under System Settings -> Privacy & Security, then restart `kasl watch`. Without them the OS silently withholds keyboard/mouse events.

### macOS: autostart does not run at login

`kasl autostart enable` installs a LaunchAgent at `~/Library/LaunchAgents/com.lacodda.kasl.plist`. Reload it manually if needed:

```bash
launchctl unload ~/Library/LaunchAgents/com.lacodda.kasl.plist
launchctl load ~/Library/LaunchAgents/com.lacodda.kasl.plist
```

### Linux: no activity detected

The activity monitor reads raw input devices. Add your user to the `input` group and re-login:

```bash
sudo usermod -a -G input $USER
ls -la /dev/input/
```

### Linux: autostart does not run at login

`kasl autostart enable` installs a systemd user unit. Check it directly:

```bash
systemctl --user status kasl
systemctl --user enable --now kasl
```

## Support Channels

- **GitHub Issues**: [https://github.com/lacodda/kasl/issues](https://github.com/lacodda/kasl/issues)
- **Documentation**: [https://kasl.lacodda.com](https://kasl.lacodda.com)

## Related pages

- [Configuration](/concepts/configuration/) - all settings referenced above
- [`watch`](/reference/watch/) - the activity monitor
- [`pauses`](/reference/pauses/) - recording missed absences
- [`setup`](/reference/setup/) - creating and resetting configuration
- [`autostart`](/reference/autostart/) - platform autostart details
