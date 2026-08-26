---
title: "self-update"
---

The `self-update` command checks GitHub releases for a newer version of kasl and, if one exists, downloads it and replaces the running binary in place - keeping the previous one alongside it as `kasl.bak`.

:::note[Renamed in 1.2]
This command used to be called `update`. The old name still works and does the
same thing, printing a notice that points here; it will be removed in 2.0.
:::

## Usage

```bash
kasl self-update
```

The command takes no arguments.

## How It Works

1. **Version check**: reads the latest release tag from the `releases/latest` redirect - not the `api.github.com` endpoint, which is rate-limited to 60 anonymous requests per hour per IP and starves any machine behind a shared NAT.
2. **Platform detection**: picks the release asset matching the current OS/architecture.
3. **Download and extraction**: retrieves the archive and unpacks the binaries from it. Nothing else in the archive is written next to the executable.
4. **Replacement**: the current executable is renamed to `kasl.bak` and the new one takes its place.
5. **Alias refresh**: the short `ka` alias is updated too, where it is installed.

If the watcher is running, it is stopped before the swap and restarted afterward.

## Supported platforms

- `x86_64-pc-windows-msvc` - Windows 64-bit
- `x86_64-apple-darwin` - macOS Intel
- `aarch64-apple-darwin` - macOS Apple Silicon
- `x86_64-unknown-linux-gnu` - Linux 64-bit

## The `.bak` backup

There is exactly one backup, `kasl.bak`, sitting next to the binary. Each update overwrites it, so it always holds the version you were running immediately before the most recent update - never anything older. Restoring it is a manual copy over the current binary; nothing reverts automatically.

## The `ka` alias

Where the short alias is installed, `self-update` replaces it along with the main binary, so `ka` never answers to an older version than `kasl`. An update never adds the alias to an installation that does not have it - skipping it at install time (`KASL_NO_ALIAS=1`) stays skipped.

On macOS and Linux the installer makes `ka` a symlink to `kasl`, so it follows every update for free.

## Sample Output

The download and binary swap print nothing; only the result line does:

```
ℹ️ No update required. You are using the latest version!
```

or, when a newer release was installed:

```
✅ The kasl application has been successfully updated to version X.Y.Z!
```

`X.Y.Z` is the version that was just installed - the message always names the actual new version, not a fixed number.

If the watcher was running, two extra lines bracket the result:

```
ℹ️ Stopping watcher for update...
ℹ️ Restarting watcher after update...
```

Other commands print a similar notice when they notice a newer release is available:

```
A new version of kasl is available: v1.5.0
Upgrade now by running: kasl self-update
```

## Related commands

- [`autostart`](/reference/autostart/) - autostart settings are preserved during updates
- [`setup`](/reference/setup/) - configuration is maintained during updates
- [`watch`](/reference/watch/) - monitoring continues after a successful update
