# `autostart` Command

The `autostart` command provides cross-platform functionality for managing whether kasl automatically starts monitoring when the system boots. It supports different autostart mechanisms depending on the operating system.

## Usage

```bash
kasl autostart [COMMAND]
```

## Commands

### `enable` - Enable autostart on system boot

```bash
kasl autostart enable
```

Configures the system to automatically start kasl monitoring when the user logs in or the system boots.

**Platform-specific behavior:**

**Windows:**
- **Primary**: Windows Task Scheduler (requires admin privileges)
- **Fallback**: Registry Run key (current user)
- Provides clear feedback about which method was used

**macOS**: LaunchAgent (planned - not yet implemented)
**Linux**: systemd user service (planned - not yet implemented)

### `disable` - Disable autostart on system boot

```bash
kasl autostart disable
```

Removes any existing autostart configuration, ensuring kasl will not automatically start on system boot.

### `status` - Show current autostart status

```bash
kasl autostart status
```

Checks and displays whether autostart is currently enabled or disabled on the system.

## Examples

### Basic Usage

```bash
# Enable autostart
kasl autostart enable

# Check current status
kasl autostart status

# Disable autostart
kasl autostart disable
```

### Windows-Specific Examples

```bash
# Enable with admin privileges (recommended)
# Run as Administrator
kasl autostart enable

# Enable without admin (user-level only)
kasl autostart enable

# Check which method was used
kasl autostart status
```

## Platform Support

### Windows

**Supported Methods:**
1. **Windows Task Scheduler** (Primary)
   - Requires administrator privileges
   - More reliable and configurable
   - Can be managed through Windows Task Manager

2. **Registry Run Key** (Fallback)
   - User-level configuration
   - Works without admin privileges
   - Less configurable than Task Scheduler

**Implementation Details:**
- Attempts Task Scheduler first
- Falls back to Registry if admin access denied
- Provides clear feedback about which method was used

### macOS (Planned)

**Future Implementation:**
- **LaunchAgent**: User-level autostart
- Configuration in `~/Library/LaunchAgents/`
- Integration with macOS system preferences

### Linux (Planned)

**Future Implementation:**
- **systemd user service**: Modern Linux systems
- **init.d scripts**: Legacy systems
- Integration with desktop environment autostart

## Sample Output

### Enable Autostart (Windows - Task Scheduler)
```
✅ Autostart enabled successfully!
Method: Windows Task Scheduler
Status: Active
Next run: On system startup
```

### Enable Autostart (Windows - Registry Fallback)
```
⚠️  Autostart enabled with limited functionality
Method: Registry Run Key (user-level)
Status: Active
Note: Task Scheduler method requires administrator privileges
```

### Status Check
```
Autostart Status: Enabled
Method: Windows Task Scheduler
Configuration: Active
Next startup: Will start automatically on system boot
```

### Disable Autostart
```
✅ Autostart disabled successfully!
Removed: Task Scheduler entry
Removed: Registry entries
Status: Disabled
```

### Unsupported Platform
```
❌ Autostart not supported on this platform
Platform: macOS
Status: Not implemented
Note: This feature is planned for future releases
```

## Use Cases

### Daily Workflow Automation

```bash
# Set up autostart for seamless monitoring
kasl autostart enable

# Verify it's working
kasl autostart status

# Start monitoring manually for first time
kasl watch
```

### System Administration

```bash
# Deploy to multiple workstations
kasl autostart enable

# Verify deployment
kasl autostart status

# Remove from decommissioned systems
kasl autostart disable
```

### Development and Testing

```bash
# Enable for testing
kasl autostart enable

# Test autostart functionality
# Restart system and verify kasl starts

# Disable after testing
kasl autostart disable
```

## Troubleshooting

### Common Issues

**Insufficient privileges (Windows)**
```bash
# Run as Administrator
# Right-click Command Prompt/PowerShell -> "Run as Administrator"
kasl autostart enable

# Or use user-level fallback
kasl autostart enable
# Will use Registry method automatically
```

**Autostart not working after enable**
```bash
# Check status
kasl autostart status

# Verify system startup
# Restart system and check if kasl starts

# Re-enable if needed
kasl autostart disable
kasl autostart enable
```

**Platform not supported**
```bash
# Check current platform support
kasl autostart status

# Manual startup alternative
# Add kasl watch to your startup scripts
```

### Windows-Specific Issues

**Task Scheduler not working**
```bash
# Check Task Scheduler manually
# Open Task Scheduler -> Task Scheduler Library -> kasl

# Try Registry method
kasl autostart disable
kasl autostart enable
```

**Registry method issues**
```bash
# Check Registry manually
# HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run

# Re-enable with admin privileges
kasl autostart disable
# Run as Administrator
kasl autostart enable
```

### Verification Steps

```bash
# 1. Enable autostart
kasl autostart enable

# 2. Check status
kasl autostart status

# 3. Test by restarting system
# Restart your computer

# 4. Verify kasl is running
kasl watch --stop
# If kasl was running, autostart is working
```

## Best Practices

### Windows Deployment

1. **Use administrator privileges** when possible for Task Scheduler method
2. **Test autostart functionality** after deployment
3. **Document deployment method** used for each system
4. **Monitor autostart status** regularly

### System Management

1. **Enable autostart** on all workstations that need monitoring
2. **Disable autostart** when removing kasl from systems
3. **Verify functionality** after system updates
4. **Keep autostart configuration** documented

### Security Considerations

1. **Task Scheduler method** is more secure than Registry
2. **User-level autostart** is sufficient for most use cases
3. **Monitor autostart entries** for unauthorized changes
4. **Disable autostart** on shared systems when appropriate

## Integration with Other Commands

The `autostart` command works with other kasl commands:

- **`watch`**: The monitoring command that gets started automatically
- **`init`**: Configure kasl before enabling autostart
- **`status`**: Check if monitoring is running after autostart

## Related Commands

- **[`watch`](./watch.md)** - The monitoring command that autostart enables
- **[`init`](./init.md)** - Configure kasl before setting up autostart
- **[`update`](./update.md)** - Update kasl while preserving autostart settings
