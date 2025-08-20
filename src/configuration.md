# Configuration

kasl uses a JSON configuration file to store all application settings. This guide covers all configuration options and their usage.

## Configuration File Location

Configuration files are stored in platform-specific locations:

- **Windows**: `%LOCALAPPDATA%\lacodda\kasl\config.json`
- **macOS**: `~/Library/Application Support/lacodda/kasl/config.json`
- **Linux**: `~/.local/share/lacodda/kasl/config.json`

## Configuration Structure

```json
{
  "monitor": {
    "min_pause_duration": 20,
    "pause_threshold": 60,
    "poll_interval": 500,
    "activity_threshold": 30,
    "min_work_interval": 10
  },
  "si": {
    "login": "your.email@company.com",
    "auth_url": "https://auth.company.com",
    "api_url": "https://api.company.com"
  },
  "gitlab": {
    "access_token": "your-token",
    "api_url": "https://gitlab.com"
  },
  "jira": {
    "login": "your.email@company.com",
    "api_url": "https://jira.company.com"
  },
  "server": {
    "api_url": "https://api.company.com/timetracking",
    "auth_token": "your-api-token"
  }
}
```

## Monitor Configuration

Controls activity monitoring behavior:

### `min_pause_duration`
- **Type**: `u64`
- **Default**: `20`
- **Unit**: Minutes
- **Description**: Minimum break duration to record in the database
- **Usage**: Pauses shorter than this threshold are ignored

### `pause_threshold`
- **Type**: `u64`
- **Default**: `60`
- **Unit**: Seconds
- **Description**: Inactivity duration before a pause is detected
- **Usage**: Time without keyboard/mouse activity to trigger pause

### `poll_interval`
- **Type**: `u64`
- **Default**: `500`
- **Unit**: Milliseconds
- **Description**: Frequency of activity status checks
- **Usage**: Lower values = more responsive, higher CPU usage

### `activity_threshold`
- **Type**: `u64`
- **Default**: `30`
- **Unit**: Seconds
- **Description**: Continuous activity required to start a workday
- **Usage**: Prevents false starts from brief interactions

### `min_work_interval`
- **Type**: `u64`
- **Default**: `10`
- **Unit**: Minutes
- **Description**: Minimum work interval for merging
- **Usage**: Short intervals are merged with adjacent ones

## SiServer Configuration

Internal company API integration:

### `login`
- **Type**: `String`
- **Description**: Corporate username for LDAP authentication
- **Example**: `"john.doe@company.com"`

### `auth_url`
- **Type**: `String`
- **Description**: Authentication endpoint URL
- **Example**: `"https://auth.company.com"`

### `api_url`
- **Type**: `String`
- **Description**: Main API endpoint URL
- **Example**: `"https://api.company.com"`

## GitLab Configuration

GitLab API integration for commit tracking:

### `access_token`
- **Type**: `String`
- **Description**: Personal Access Token with required scopes
- **Required Scopes**: `read_user`, `read_repository`
- **Generation**: GitLab → User Settings → Access Tokens

### `api_url`
- **Type**: `String`
- **Description**: GitLab instance base URL
- **Examples**:
  - `"https://gitlab.com"` (GitLab.com)
  - `"https://gitlab.company.com"` (Self-hosted)

## Jira Configuration

Jira API integration for issue tracking:

### `login`
- **Type**: `String`
- **Description**: Jira username (not email unless configured)
- **Note**: Check with Jira administrator for username format

### `api_url`
- **Type**: `String`
- **Description**: Jira instance base URL
- **Examples**:
  - `"https://company.atlassian.net"` (Atlassian Cloud)
  - `"https://jira.company.com"` (Server/Data Center)

## Server Configuration

External reporting API configuration:

### `api_url`
- **Type**: `String`
- **Description**: Base URL for report submission
- **Example**: `"https://api.company.com/timetracking"`

### `auth_token`
- **Type**: `String`
- **Description**: Authentication token for API access
- **Format**: Depends on API requirements (Bearer, API key, etc.)

## Interactive Configuration

### Initial Setup

Run the interactive configuration wizard:

```bash
kasl init
```

This guides you through:
1. Monitor settings configuration
2. API integration setup
3. Server configuration
4. Credential management

### Configuration Reset

Remove existing configuration:

```bash
kasl init --delete
```

This will:
- Delete the configuration file
- Remove global PATH settings
- Reset to initial state

## Manual Configuration

### Creating Configuration File

Create the configuration directory and file:

```bash
# Windows
mkdir "%LOCALAPPDATA%\lacodda\kasl"

# macOS/Linux
mkdir -p ~/.local/share/lacodda/kasl
```

### Example Configuration

```json
{
  "monitor": {
    "min_pause_duration": 15,
    "pause_threshold": 45,
    "poll_interval": 1000,
    "activity_threshold": 60,
    "min_work_interval": 5
  },
  "gitlab": {
    "access_token": "glpat-XXXXXXXXXXXXXXXXXXXX",
    "api_url": "https://gitlab.com"
  },
  "jira": {
    "login": "john.doe",
    "api_url": "https://company.atlassian.net"
  }
}
```

## Configuration Validation

### Syntax Check

Validate JSON syntax:

```bash
# Using jq (if available)
jq . config.json

# Using Python
python -m json.tool config.json
```

### Runtime Validation

kasl validates configuration on startup:

```bash
kasl watch --foreground
```

Common validation errors:
- Invalid JSON syntax
- Missing required fields
- Invalid URL formats
- Unsupported configuration values

## Security Considerations

### Credential Storage

- **API Tokens**: Stored encrypted in separate files
- **Passwords**: Prompted interactively, not stored
- **Session Data**: Cached temporarily for performance

### File Permissions

Ensure proper file permissions:

```bash
# Linux/macOS
chmod 600 ~/.local/share/lacodda/kasl/config.json
chmod 700 ~/.local/share/lacodda/kasl/
```

### Environment Variables

Override configuration with environment variables:

```bash
# Override monitor settings
export KASL_MIN_PAUSE_DURATION=30
export KASL_PAUSE_THRESHOLD=90

# Override API URLs
export KASL_GITLAB_API_URL=https://gitlab.company.com
export KASL_JIRA_API_URL=https://jira.company.com
```

## Troubleshooting

### Configuration Issues

**Problem**: Configuration not found
```bash
# Check if file exists
ls ~/.local/share/lacodda/kasl/config.json

# Recreate configuration
kasl init
```

**Problem**: Invalid configuration
```bash
# Validate JSON syntax
python -m json.tool config.json

# Check for missing fields
kasl watch --foreground
```

**Problem**: API connection failures
```bash
# Test API connectivity
curl -H "Authorization: Bearer YOUR_TOKEN" https://api.company.com/health

# Check network settings
ping api.company.com
```

### Debug Configuration

Enable debug logging to see configuration loading:

```bash
RUST_LOG=kasl=debug kasl watch --foreground
```

This will show:
- Configuration file location
- Loaded configuration values
- Validation results
- API connection attempts
