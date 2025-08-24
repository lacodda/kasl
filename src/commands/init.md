# `init` - Initialize Application Configuration

The `init` command sets up kasl configuration interactively, guiding you through the initial setup process.

## Usage

```bash
kasl init [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `-d, --delete` | Remove existing configuration instead of creating new one |
| `-h, --help` | Print help information |

## Description

The `init` command provides an interactive configuration wizard that guides you through setting up:

- **Monitor Settings**: Activity detection thresholds and timing
- **API Integrations**: GitLab, Jira, and SiServer connections
- **Server Configuration**: External reporting endpoints
- **Credential Management**: Secure storage setup

## Interactive Setup

### Monitor Configuration

Configure activity monitoring behavior:

```
Minimum pause duration (minutes) [20]: 
Pause threshold (seconds) [60]: 
Poll interval (milliseconds) [500]: 
Activity threshold (seconds) [30]: 
Minimum work interval (minutes) [10]: 
```

**Settings Explained**:
- **Minimum pause duration**: Breaks shorter than this are ignored
- **Pause threshold**: Time without activity before pause detection
- **Poll interval**: How often to check for activity
- **Activity threshold**: Continuous activity needed to start workday
- **Minimum work interval**: Short intervals to merge

### API Integrations

Configure external service connections:

#### GitLab Integration
```
Enable GitLab integration? (y/N): y
GitLab API URL [https://gitlab.com]: 
GitLab Access Token: 
```

#### Jira Integration
```
Enable Jira integration? (y/N): y
Jira API URL: 
Jira Username: 
```

#### SiServer Integration
```
Enable SiServer integration? (y/N): y
SiServer Auth URL: 
SiServer API URL: 
SiServer Username: 
```

### Server Configuration

Configure external reporting:

```
Enable external reporting? (y/N): y
Server API URL: 
Authentication Token: 
```

## Configuration Reset

Remove existing configuration:

```bash
kasl init --delete
```

This will:
- Delete the configuration file
- Remove global PATH settings
- Reset to initial state

## Configuration File Location

Configuration is stored in platform-specific locations:

- **Windows**: `%LOCALAPPDATA%\lacodda\kasl\config.json`
- **macOS**: `~/Library/Application Support/lacodda/kasl/config.json`
- **Linux**: `~/.local/share/lacodda/kasl/config.json`

## Example Configuration

After running `init`, your configuration will look like:

```json
{
  "monitor": {
    "min_pause_duration": 20,
    "pause_threshold": 60,
    "poll_interval": 500,
    "activity_threshold": 30,
    "min_work_interval": 10
  },
  "gitlab": {
    "access_token": "glpat-XXXXXXXXXXXXXXXXXXXX",
    "api_url": "https://gitlab.com"
  },
  "jira": {
    "login": "john.doe",
    "api_url": "https://company.atlassian.net"
  },
  "si": {
    "login": "john.doe@company.com",
    "auth_url": "https://auth.company.com",
    "api_url": "https://api.company.com"
  },
  "server": {
    "api_url": "https://api.company.com/timetracking",
    "auth_token": "your-api-token"
  }
}
```

## Credential Management

### Secure Storage

Credentials are stored securely:
- **API Tokens**: Encrypted in separate files
- **Passwords**: Prompted interactively, not stored
- **Session Data**: Cached temporarily

### File Permissions

Ensure proper file permissions:

```bash
# Linux/macOS
chmod 600 ~/.local/share/lacodda/kasl/config.json
chmod 700 ~/.local/share/lacodda/kasl/
```

## Troubleshooting

### Configuration Issues

**Problem**: Configuration not saved
```bash
# Check file permissions
ls -la ~/.local/share/lacodda/kasl/config.json

# Recreate configuration
kasl init --delete
kasl init
```

**Problem**: Invalid configuration
```bash
# Validate JSON syntax
python -m json.tool ~/.local/share/lacodda/kasl/config.json

# Reset configuration
kasl init --delete
kasl init
```

### API Configuration Issues

**Problem**: API connection failures
```bash
# Test connectivity
curl -H "Authorization: Bearer YOUR_TOKEN" https://gitlab.com/api/v4/user

# Reconfigure integration
kasl init
```

## Examples

### Basic Setup

```bash
# Run interactive setup
kasl init

# Follow prompts to configure:
# 1. Monitor settings
# 2. API integrations (optional)
# 3. Server configuration (optional)
```

### Reset Configuration

```bash
# Remove existing configuration
kasl init --delete

# Run setup again
kasl init
```

### Partial Configuration

```bash
# Run setup and skip optional integrations
kasl init

# Only configure monitor settings
# Skip GitLab, Jira, and SiServer when prompted
```

## Next Steps

After running `init`:

1. **Start monitoring**:
   ```bash
   kasl watch
   ```

2. **Enable autostart** (optional):
   ```bash
   kasl autostart enable
   ```

3. **Create your first task**:
   ```bash
   kasl task --name "Set up kasl" --completeness 100
   ```

4. **View your report**:
   ```bash
   kasl report
   ```

## Related Commands

- [`watch`](./watch.md) - Start activity monitoring
- [`task`](./task.md) - Manage tasks
- [`report`](./report.md) - Generate reports
- [`autostart`](./autostart.md) - Configure automatic startup

