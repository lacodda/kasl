# API Integrations

kasl supports integration with external services for enhanced task management and reporting capabilities.

## Overview

API integrations provide:
- **Task Discovery**: Import tasks from external systems
- **Report Submission**: Send reports to organizational systems
- **Credential Management**: Secure storage and authentication
- **Session Handling**: Automatic session management and renewal

## GitLab Integration

Import commits and merge requests as completed tasks.

### Setup

1. **Generate Access Token**:
   - Go to GitLab → User Settings → Access Tokens
   - Create token with scopes: `read_user`, `read_repository`
   - Copy the generated token

2. **Configure Integration**:
   ```bash
   kasl init
   # Follow prompts to configure GitLab
   ```

### Configuration

```json
{
  "gitlab": {
    "access_token": "glpat-XXXXXXXXXXXXXXXXXXXX",
    "api_url": "https://gitlab.com"
  }
}
```

### Features

- **Commit Import**: Automatically import today's commits as completed tasks
- **User Activity**: Track user activity across repositories
- **Repository Filtering**: Support for multiple repositories
- **Commit Message Parsing**: Extract meaningful task names from commit messages

### Usage

```bash
# Find tasks from GitLab
kasl task --find

# This will show:
# - Incomplete local tasks
# - Today's GitLab commits
# - Interactive selection interface
```

### API Endpoints Used

- `GET /api/v4/user` - Get user information
- `GET /api/v4/events` - Get user events
- `GET /api/v4/projects/{id}/repository/commits/{sha}` - Get commit details

## Jira Integration

Import completed issues and track work items.

### Setup

1. **Get Credentials**:
   - Username (not email, unless configured)
   - Password (prompted interactively)
   - Jira instance URL

2. **Configure Integration**:
   ```bash
   kasl init
   # Follow prompts to configure Jira
   ```

### Configuration

```json
{
  "jira": {
    "login": "john.doe",
    "api_url": "https://company.atlassian.net"
  }
}
```

### Features

- **Issue Import**: Import completed issues as tasks
- **Status Tracking**: Filter by issue status
- **Project Support**: Support for multiple projects
- **Field Mapping**: Custom field support

### Usage

```bash
# Find tasks from Jira
kasl task --find

# This will show:
# - Incomplete local tasks
# - Today's GitLab commits
# - Today's completed Jira issues
# - Interactive selection interface
```

### API Endpoints Used

- `POST /rest/auth/1/session` - Authenticate
- `GET /rest/api/2/search` - Search issues
- `GET /rest/api/2/issue/{key}` - Get issue details

## SiServer Integration

Submit reports to internal company systems.

### Setup

1. **Get Credentials**:
   - Corporate username
   - Password (prompted interactively)
   - Authentication URL
   - API URL

2. **Configure Integration**:
   ```bash
   kasl init
   # Follow prompts to configure SiServer
   ```

### Configuration

```json
{
  "si": {
    "login": "john.doe@company.com",
    "auth_url": "https://auth.company.com",
    "api_url": "https://api.company.com"
  }
}
```

### Features

- **Daily Reports**: Submit daily work reports
- **Monthly Reports**: Submit monthly summaries
- **Rest Dates**: Import company holidays and rest days
- **Secure Authentication**: LDAP-based authentication

### Usage

```bash
# Submit daily report
kasl report --send

# Submit monthly report
kasl sum --send

# This will:
# - Authenticate with SiServer
# - Format report data
# - Submit via API
# - Handle errors and retries
```

### API Endpoints Used

- `POST /auth/login` - Authenticate
- `POST /reports/daily` - Submit daily report
- `POST /reports/monthly` - Submit monthly report
- `GET /calendar/rest-dates` - Get rest dates

## Credential Management

### Secure Storage

Credentials are stored securely:

- **API Tokens**: Encrypted in separate files
- **Passwords**: Not stored, prompted interactively
- **Session Data**: Cached temporarily

### File Locations

- **Windows**: `%LOCALAPPDATA%\lacodda\kasl\`
- **macOS**: `~/Library/Application Support/lacodda/kasl/`
- **Linux**: `~/.local/share/lacodda/kasl/`

### Encryption

- **Algorithm**: AES-256-CBC
- **Key Management**: Compile-time embedded keys
- **File Permissions**: Restricted access

## Session Management

### Automatic Handling

Sessions are managed automatically:

- **Authentication**: Prompted when needed
- **Caching**: Sessions cached for performance
- **Renewal**: Automatic session renewal
- **Cleanup**: Expired sessions removed

### Session Files

- `.gitlab_session` - GitLab session data
- `.jira_session` - Jira session data
- `.si_session` - SiServer session data

### Error Handling

- **Network Errors**: Automatic retry with backoff
- **Authentication Failures**: Re-prompt for credentials
- **Session Expiry**: Automatic re-authentication
- **Rate Limiting**: Respect API rate limits

## Troubleshooting

### Common Issues

**Problem**: Authentication failures
```bash
# Clear cached sessions
rm ~/.local/share/lacodda/kasl/.gitlab_session
rm ~/.local/share/lacodda/kasl/.jira_session
rm ~/.local/share/lacodda/kasl/.si_session

# Reconfigure integration
kasl init
```

**Problem**: API connection errors
```bash
# Test connectivity
curl -H "Authorization: Bearer YOUR_TOKEN" https://gitlab.com/api/v4/user

# Check network settings
ping gitlab.com
```

**Problem**: Rate limiting
```bash
# Wait and retry
# kasl handles rate limiting automatically
# Check API documentation for limits
```

### Debug Mode

Enable debug logging for API operations:

```bash
RUST_LOG=kasl=debug kasl task --find
```

This will show:
- API requests and responses
- Authentication attempts
- Session management
- Error details

### API Limits

Be aware of API rate limits:

- **GitLab**: 600 requests/hour for authenticated users
- **Jira**: Varies by plan and usage
- **SiServer**: Depends on company configuration

## Best Practices

### Security

- **Token Rotation**: Regularly rotate API tokens
- **Minimal Permissions**: Use tokens with minimal required scopes
- **Secure Storage**: Keep configuration files secure
- **Network Security**: Use HTTPS for all API communications

### Performance

- **Caching**: Sessions are cached to reduce API calls
- **Batch Operations**: Use batch operations when possible
- **Rate Limiting**: Respect API rate limits
- **Connection Pooling**: Efficient HTTP connection management

### Monitoring

- **Error Tracking**: Monitor for authentication failures
- **Usage Monitoring**: Track API usage patterns
- **Performance Metrics**: Monitor response times
- **Log Analysis**: Review debug logs for issues

## Custom Integrations

### Adding New APIs

To add support for new APIs:

1. **Create API Client**: Implement the `Session` trait
2. **Add Configuration**: Extend configuration structure
3. **Update Commands**: Add integration to relevant commands
4. **Add Tests**: Comprehensive test coverage

### Example Implementation

```rust
pub struct CustomApi {
    client: Client,
    config: CustomConfig,
    credentials: Option<LoginCredentials>,
    retries: i32,
}

impl Session for CustomApi {
    async fn login(&self) -> Result<String> {
        // Implementation
    }
    
    fn set_credentials(&mut self, password: &str) -> Result<()> {
        // Implementation
    }
    
    // ... other required methods
}
```

### Configuration Extension

```json
{
  "custom_api": {
    "login": "username",
    "api_url": "https://api.example.com"
  }
}
```
