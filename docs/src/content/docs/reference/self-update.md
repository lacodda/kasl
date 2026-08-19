---
title: "self-update"
---

The `self-update` command handles checking for and installing newer versions of kasl from GitHub releases. It replaces the binary in place, keeping the previous one alongside it as `kasl.bak`.

:::note[Renamed in 1.2]
This command used to be called `update`. The old name still works and does the
same thing, printing a notice that points here; it will be removed in 2.0.
:::

## Usage

```bash
kasl self-update
```

## How It Works

The update process performs a complete workflow:

1. **Version Check**: Reads the latest release tag from the `releases/latest` redirect (no GitHub API quota involved)
2. **Platform Detection**: Identifies the correct binary for the current OS/architecture
3. **Download**: Retrieves the latest release archive
4. **Extraction**: Unpacks the new binaries from the archive
5. **Replacement**: Safely replaces the current executable with backup
6. **Alias refresh**: Updates the short `ka` alias too, where it is installed

## Update Sources

Updates are fetched from GitHub releases at:
`https://github.com/{owner}/{repo}/releases/latest`

The updater automatically selects the appropriate asset based on:
- **Architecture**: x86_64, aarch64, etc.
- **Operating System**: Windows (MSVC), macOS (Darwin), Linux (glibc)

## Platform Support

Supported platform identifiers:
- `x86_64-pc-windows-msvc` - Windows 64-bit
- `x86_64-apple-darwin` - macOS Intel
- `aarch64-apple-darwin` - macOS Apple Silicon
- `x86_64-unknown-linux-gnu` - Linux 64-bit

## Safety Features

The update process is designed to be safe and atomic:

- **Backup Creation**: Creates backups of the current executable before replacement
- **Archive Validation**: Validates downloaded archives before extraction
- **Quiet Until Done**: The download and swap print nothing; the result line says which version is now installed
- **Error Handling**: Handles network errors and other issues gracefully
- **Manual Rollback**: The replaced binary is kept as `kasl.bak`, so a bad update can be undone by hand
- **No Leftovers**: Only the executables are taken out of the archive; nothing else is written next to the binary

## Examples

### Basic Update

```bash
# Check for and install updates
kasl self-update
```

### Update Workflow

```bash
# 1. Check current version
kasl --version

# 2. Update to latest version
kasl self-update

# 3. Verify update
kasl --version
```

## Sample Output

### Already Up To Date
```
ℹ️ No update required. You are using the latest version!
```

### Update Installed
```
✅ The kasl application has been successfully updated to version 1.2.0!
```

The download and the binary swap are silent; only the result is printed.

### With The Watcher Running

The daemon holds the executable, so it is stopped first and started again
afterwards:

```
ℹ️ Stopping watcher for update...
ℹ️ Restarting watcher after update...
✅ The kasl application has been successfully updated to version 1.2.0!
```

### Update Available

Other commands mention a newer release when they notice one:

```
A new version of kasl is available: v1.2.0
Upgrade now by running: kasl self-update
```

## Use Cases

### Regular Maintenance

```bash
# Check for updates weekly
kasl self-update

# Verify update was successful
kasl --version
```

### System Administration

```bash
# Update kasl on multiple systems
for system in system1 system2 system3; do
    ssh $system "kasl self-update"
done
```

### Development and Testing

```bash
# Update to latest development version
kasl self-update

# Test new features
kasl --help

# Rollback if needed (manual process)
# Restore from backup file
```

## Troubleshooting

### Common Issues

**Network connectivity problems**
```bash
# Check internet connection
ping github.com

# Try again later
kasl self-update
```

**Permission errors**
```bash
# Run with elevated privileges (Windows)
# Right-click Command Prompt -> "Run as Administrator"
kasl self-update

# Check file permissions (Unix)
ls -la $(which kasl)
```

**Insufficient disk space**
```bash
# Check available disk space
df -h

# Clean up space and try again
kasl self-update
```

### Update Failures

**Download failed**
```bash
# Check network connection
curl -I https://github.com

# Try again
kasl self-update
```

**Extraction failed**
```bash
# Check if backup exists
ls -la kasl.bak

# Restore from backup manually
cp kasl.bak kasl.exe
```

**Binary replacement failed**
```bash
# Check if kasl is running
tasklist | grep kasl

# Stop kasl and try again
kasl watch --stop
kasl self-update
```

### Verification Steps

```bash
# 1. Check current version
kasl --version

# 2. Perform update
kasl self-update

# 3. Verify new version
kasl --version

# 4. Test functionality
kasl --help
```

## Best Practices

### Update Strategy

1. **Regular updates**: Check for updates weekly or monthly
2. **Test after update**: Verify functionality after each update
3. **Keep backups**: Don't delete backup files immediately
4. **Monitor for issues**: Watch for any problems after updates

### System Management

1. **Update during maintenance windows**: Choose appropriate times for updates
2. **Update all systems**: Keep all installations on the same version
3. **Document update process**: Keep track of update procedures
4. **Test in staging**: Test updates on non-critical systems first

### Backup Management

There is exactly one backup, `kasl.bak`, sitting next to the binary. Each
update overwrites it, so it always holds the version you were running before
the most recent update - and nothing older.

1. **Keep it until the new version is proven**: it is the only way back
2. **Restore by copying it over the binary**: nothing reverts automatically

## The `ka` Alias

Where the short alias is installed, `self-update` replaces it along with the
main binary, so `ka` never answers to an older version than `kasl`. An update
never adds the alias to an installation that does not have it - skipping it at
install time (`KASL_NO_ALIAS=1`) stays skipped.

On macOS and Linux the installer makes `ka` a symlink to `kasl`, so it follows
every update for free.

## Integration with Other Commands

The `self-update` command works with other kasl commands:

- **`autostart`**: Updates preserve autostart configuration
- **`setup`**: Configuration settings are preserved during updates
- **`watch`**: Monitoring continues after update (if autostart is enabled)

## Related Commands

- **[`autostart`](/reference/autostart/)** - Autostart settings are preserved during updates
- **[`setup`](/reference/setup/)** - Configuration is maintained during updates
- **[`watch`](/reference/watch/)** - Monitoring continues after successful updates
