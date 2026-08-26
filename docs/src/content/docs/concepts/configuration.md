---
title: "Configuration"
---

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
  },
  "productivity": {
    "min_productivity_threshold": 75.0,
    "workday_hours": 8.0,
    "min_workday_fraction_before_suggest": 0.5
  },
  "task_discovery": {
    "ignore_names": [
      "Merge remote-tracking branch",
      "Merge branch ",
      "update webui"
    ]
  }
}
```

## Task Discovery Configuration

Controls filtering for `kasl task find`:

### `ignore_names`
- **Type**: `string[]`
- **Default**: `["Merge remote-tracking branch", "Merge branch ", "update webui"]`
- **Description**: Task/commit names (or prefixes) excluded from discovery
- **Matching**: Case-insensitive after normalization; exact match or prefix
- **UI**: Edit via `kasl setup` (Task discovery module) or add items from `kasl task find`

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
- **Description**: Minimum work interval duration for report filtering
- **Usage**: Intervals shorter than this duration are automatically filtered out from reports (display and API submission)

### `pause_merge_gap`
- **Type**: `u64`
- **Default**: `30`
- **Unit**: Seconds
- **Description**: Largest gap between two consecutive pauses that still counts as one pause
- **Usage**: A stray keypress in the middle of a break splits it into two records; pauses no further apart than this are treated as one, so the sub-threshold halves are not dropped. Keep it small - a few tens of seconds - or genuine short work periods get swallowed into the break. Not offered by the wizard; edit `config.json` to change it

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

## Productivity Configuration

Controls productivity tracking and reporting thresholds:

### `min_productivity_threshold`
- **Type**: `f64`
- **Default**: `75.0`
- **Description**: Minimum productivity percentage required for report submission
- **Range**: `0.0` to `100.0`
- **Usage**: Reports below this threshold are blocked. If an absence is missing from the day, record it with `kasl pauses add`

### `workday_hours`
- **Type**: `f64`
- **Default**: `8.0`
- **Description**: Expected daily work hours for productivity calculations
- **Range**: `1.0` to `24.0`
- **Usage**: Used to calculate available work time for productivity metrics

### `min_workday_fraction_before_suggest`
- **Type**: `f64`
- **Default**: `0.5`
- **Description**: Fraction of workday that must pass before the low-productivity warning appears
- **Range**: `0.0` to `1.0`
- **Usage**: Early in the day the productivity ratio swings on a single pause, so warning then would be noise

**Example Configuration:**
```json
{
  "productivity": {
    "min_productivity_threshold": 75.0,
    "workday_hours": 8.0,
    "min_workday_fraction_before_suggest": 0.5
  }
}
```

## Report Configuration

Defaults for generated report files. Every field is optional.

### `output_dir`
- **Type**: `String`
- **Default**: unset
- **Description**: Directory for exports made without an explicit `--output`; created if missing
- **Usage**: Unset means a timestamped file in the current directory

### `filename_template`
- **Type**: `String`
- **Default**: `"daily_report_{date}{seq}"`
- **Description**: File name (without extension) for generated reports
- **Placeholders**: `{date}` - the report date as `YYYY-MM-DD`; `{seq}` - empty for the day's first report, then `_2`, `_3`, and so on
- **Usage**: The extension follows the export format

### `language`
- **Type**: `String`
- **Default**: `"en"`
- **Values**: `"en"` or `"ru"`; anything else falls back to `"en"`
- **Description**: Language of the labels inside hourly Excel reports - the title, the weekday, the break label and the column headers
- **Usage**: Russian was the default before 1.0; this opts back into it

### `template`
- **Type**: `String`
- **Default**: unset (the built-in `siserver` look)
- **Description**: Design template name, read from `<data>/report_templates/<name>.json`
- **Usage**: A missing file falls back to the built-in look rather than failing

**Example:**
```json
{
  "report": {
    "output_dir": "~/reports",
    "filename_template": "daily_report_{date}{seq}",
    "language": "en"
  }
}
```

## Jira Inbox Configuration

Controls the background poll behind [`kasl inbox`](/reference/inbox/). Requires
the `jira` block; the inbox stays off when this one is absent.

### `enabled`
- **Type**: `bool`
- **Default**: `true`
- **Description**: Whether the watcher polls Jira for open assigned issues

### `poll_interval_secs`
- **Type**: `u64`
- **Default**: `300`
- **Unit**: Seconds
- **Description**: Time between polls

### `notify`
- **Type**: `bool`
- **Default**: `true`
- **Description**: Desktop toast when a new issue lands on you

### `notify_changes`
- **Type**: `bool`
- **Default**: `true`
- **Description**: Toast when an issue already in the inbox visibly changes - status, priority or score

### `notify_gone`
- **Type**: `bool`
- **Default**: `false`
- **Description**: Toast when an issue leaves the inbox, closed or reassigned. Off by default: a departure is rarely something to interrupt you for

### `custom_fields`
- **Type**: `{ "id": String, "label": String }[]`
- **Default**: `[]`
- **Description**: Extra Jira fields to fetch and display, such as a scoring field
- **Example**: `{ "id": "customfield_12345", "label": "Scoring" }`

### `sort_by_field`
- **Type**: `String`
- **Default**: unset
- **Description**: Field id used to rank the inbox, descending - typically the scoring custom field

**Example:**
```json
{
  "jira_inbox": {
    "enabled": true,
    "poll_interval_secs": 300,
    "notify": true,
    "notify_changes": true,
    "notify_gone": false,
    "custom_fields": [{ "id": "customfield_12345", "label": "Scoring" }],
    "sort_by_field": "customfield_12345"
  }
}
```

## Interactive Configuration

### Initial Setup

Run the interactive configuration wizard:

```bash
kasl setup
```

This guides you through:
1. Monitor settings configuration
2. API integration setup
3. Server configuration
4. Credential management

### Configuration Reset

Remove existing configuration:

```bash
kasl setup --delete
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

## Credentials are not in this file

Passwords are never written to `config.json`; they live in the OS keyring - see
[API Integrations](/concepts/api-integrations/). The file does hold two
non-password secrets as written: the GitLab `access_token` and the reporting
server's `auth_token`. On Linux and macOS it is worth keeping the file to
yourself:

```bash
chmod 600 ~/.local/share/lacodda/kasl/config.json
```

There is no environment-variable override: kasl reads its settings from this
file only.

## Editing the file

`kasl setup` walks through the modules and rewrites the file. Fields the wizard
does not offer - `pause_merge_gap` among them - are edited by hand; kasl keeps
values it did not ask about.

An unreadable or malformed file surfaces on the next command that needs it. To
see what was loaded:

```bash
RUST_LOG=kasl=debug kasl watch --foreground
```

## Related pages

- [`setup`](/reference/setup/) - the wizard that writes this file
- [API Integrations](/concepts/api-integrations/) - GitLab, Jira and SiServer in detail
- [`watch`](/reference/watch/) - what the monitor settings govern
