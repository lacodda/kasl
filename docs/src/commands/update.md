# `update` Command

The `update` command handles checking for and installing newer versions of kasl from GitHub releases. It provides automatic binary replacement with backup and rollback capabilities.

## Usage

```bash
kasl update
```

## How It Works

The update process performs a complete workflow:

1. **Version Check**: Queries GitHub API for the latest release
2. **Platform Detection**: Identifies the correct binary for the current OS/architecture
3. **Download**: Retrieves the latest release archive
4. **Extraction**: Unpacks the new binary from the archive
5. **Replacement**: Safely replaces the current executable with backup

## Update Sources

Updates are fetched from GitHub releases at:
`https://github.com/{owner}/{repo}/releases/latest`

The updater automatically selects the appropriate asset based on:
- **Architecture**: x86_64, aarch64, etc.
- **Operating System**: Windows (MSVC), macOS (Darwin), Linux (musl)

## Platform Support

Supported platform identifiers:
- `x86_64-pc-windows-msvc` - Windows 64-bit
- `x86_64-apple-darwin` - macOS Intel
- `aarch64-apple-darwin` - macOS Apple Silicon
- `x86_64-unknown-linux-musl` - Linux 64-bit

## Safety Features

The update process is designed to be safe and atomic:

- **Backup Creation**: Creates backups of the current executable before replacement
- **Archive Validation**: Validates downloaded archives before extraction
- **Clear Feedback**: Provides detailed information about the update process
- **Error Handling**: Handles network errors and other issues gracefully
- **Rollback Capability**: Can restore from backup if update fails

## Examples

### Basic Update

```bash
# Check for and install updates
kasl update
```

### Update Workflow

```bash
# 1. Check current version
kasl --version

# 2. Update to latest version
kasl update

# 3. Verify update
kasl --version
```

## Sample Output

### No Update Required
```
ℹ️  kasl is already up to date
Current version: 0.8.0
Latest version: 0.8.0
No update required.
```

### Update Available
```
🔄 Checking for updates...
✅ New version available: 0.8.1

📥 Downloading update...
├── Platform: x86_64-pc-windows-msvc
├── Size: 2.5 MB
└── Progress: 100%

🔧 Installing update...
├── Creating backup: kasl.exe.backup
├── Extracting binary
└── Replacing executable

✅ Update completed successfully!
Version: 0.8.1
Backup: kasl.exe.backup
```

### Update Error
```
❌ Update failed: Network error
Error: Failed to download release archive
Suggestion: Check your internet connection and try again
```

## Use Cases

### Regular Maintenance

```bash
# Check for updates weekly
kasl update

# Verify update was successful
kasl --version
```

### System Administration

```bash
# Update kasl on multiple systems
for system in system1 system2 system3; do
    ssh $system "kasl update"
done
```

### Development and Testing

```bash
# Update to latest development version
kasl update

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
kasl update
```

**Permission errors**
```bash
# Run with elevated privileges (Windows)
# Right-click Command Prompt -> "Run as Administrator"
kasl update

# Check file permissions (Unix)
ls -la $(which kasl)
```

**Insufficient disk space**
```bash
# Check available disk space
df -h

# Clean up space and try again
kasl update
```

### Update Failures

**Download failed**
```bash
# Check network connection
curl -I https://github.com

# Try again
kasl update
```

**Extraction failed**
```bash
# Check if backup exists
ls -la kasl.exe.backup

# Restore from backup manually
cp kasl.exe.backup kasl.exe
```

**Binary replacement failed**
```bash
# Check if kasl is running
tasklist | grep kasl

# Stop kasl and try again
kasl watch --stop
kasl update
```

### Verification Steps

```bash
# 1. Check current version
kasl --version

# 2. Perform update
kasl update

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

1. **Preserve backups**: Keep backup files for potential rollback
2. **Clean up old backups**: Remove backups older than a few versions
3. **Verify backup integrity**: Test backup restoration periodically
4. **Document rollback procedures**: Know how to restore from backup

## Integration with Other Commands

The `update` command works with other kasl commands:

- **`autostart`**: Updates preserve autostart configuration
- **`init`**: Configuration settings are preserved during updates
- **`watch`**: Monitoring continues after update (if autostart is enabled)

## Related Commands

- **[`autostart`](./autostart.md)** - Autostart settings are preserved during updates
- **[`init`](./init.md)** - Configuration is maintained during updates
- **[`watch`](./watch.md)** - Monitoring continues after successful updates
