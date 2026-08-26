---
title: "Autostart on three platforms"
sidebar:
  order: 4
---

A tracker you have to remember to start is a tracker that misses mornings.
`kasl autostart enable` hands that job to the operating system, and each of the
three does it differently enough to be worth knowing about - especially when
something does not come back after a reboot.

```bash
kasl autostart enable
kasl autostart status
kasl autostart disable
```

`status` answers `enabled` or `disabled`, and nothing more: it checks whether
the registration exists, not how it got there.

## What gets registered where

### Windows

kasl checks for administrator rights first. With them, it creates a **Task
Scheduler** entry triggered on logon and running with limited privileges.
Without them, it goes straight to a **Registry run key** under the current user
- no elevation needed, no failed attempt in between.

Both start `kasl watch`, so the practical difference is only which one you get:
run the command from an elevated prompt for the scheduled task, or from a normal
one for the per-user key.

### macOS

A **LaunchAgent** plist is written to `~/Library/LaunchAgents` and loaded
immediately with `launchctl`, so autostart takes effect without logging out.

`RunAtLoad` is set; `KeepAlive` deliberately is not. Stopping the daemon by hand
with `kasl watch --stop` stays stopped - launchd will not restart it behind your
back, and will pick it up again at the next login.

### Linux

A **systemd user unit** is written under the user configuration directory and
enabled by creating the `default.target.wants` symlink directly, rather than by
calling `systemctl --user enable`.

That detail matters when it goes wrong: a running user manager resolves units
against the configuration path it was started with, so a session where
`XDG_CONFIG_HOME` differs reports "unit file does not exist" for a file that is
plainly there. The symlink is exactly what enabling would have produced, so the
unit is honoured at the next login either way. kasl also asks a running manager
to reload, which is why autostart usually takes effect without a re-login - and
why a machine without systemd still gets a correct registration rather than an
error.

The unit is a **user** unit tied to `default.target`, not a system service. The
daemon watches one person's input and needs their session; it has no business
running as root or before anyone logs in.

## Checking that it actually worked

`kasl autostart status` confirms the registration exists. To confirm the daemon
is running after a reboot, ask kasl for the day:

```bash
kasl report
```

Intervals starting around your login time mean it worked. If nothing was
recorded, run the daemon in the foreground and watch it decide:

```bash
RUST_LOG=kasl=debug kasl watch --foreground
```

## After an update

`kasl self-update` replaces the binary in place, so a registration pointing at
that path keeps working. A registration made from a *different* copy - a build
in `target/release`, a binary you later moved - points at wherever it was when
you enabled it. Re-run `kasl autostart enable` after moving kasl, and the
registration is rewritten with the current path.

## Turning it off

```bash
kasl autostart disable
```

This removes whichever registration exists - scheduled task, registry key,
LaunchAgent or systemd unit. It does not stop a daemon that is already running;
`kasl watch --stop` does that.

## Related pages

- [`autostart`](/reference/autostart/) - the command and its subcommands
- [`watch`](/reference/watch/) - the daemon that autostart launches
- [A day with kasl](/guides/a-day-with-kasl/) - where autostart fits in the day
- [Troubleshooting](/guides/troubleshooting/) - when the daemon does not come back
