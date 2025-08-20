# Troubleshooting

This guide helps you resolve common issues with kasl.

## Common Issues

### Activity Monitoring

#### Problem: Monitoring not starting

**Symptoms**:
- `kasl watch` fails to start
- No work sessions detected
- Error messages about permissions

**Solutions**:

1. **Check permissions**:
   ```bash
   # Linux/macOS
   ls -la ~/.local/share/lacodda/kasl/
   
   # Windows
   dir "%LOCALAPPDATA%\lacodda\kasl"
   ```

2. **Run in foreground for debugging**:
   ```bash
   kasl watch --foreground
   ```

3. **Check for existing processes**:
   ```bash
   # Linux/macOS
   ps aux | grep kasl
   
   # Windows
   tasklist | findstr kasl
   ```

4. **Stop existing processes**:
   ```bash
   kasl watch --stop
   ```

#### Problem: False activity detection

**Symptoms**:
- Work sessions start unexpectedly
- Pauses not detected properly
- Inconsistent timing

**Solutions**:

1. **Adjust configuration**:
   ```bash
   kasl init  # Reconfigure monitor settings
   ```

2. **Increase thresholds**:
   ```json
   {
     "monitor": {
       "activity_threshold": 60,    // Increase from 30
       "pause_threshold": 120,      // Increase from 60
       "min_pause_duration": 30     // Increase from 20
     }
   }
   ```

3. **Check for background processes**:
   ```bash
   # Linux/macOS
   ps aux | grep -E "(mouse|keyboard|input)"
   ```

### Database Issues

#### Problem: Database locked

**Symptoms**:
- "database is locked" errors
- Cannot access data
- Application crashes

**Solutions**:

1. **Stop all kasl processes**:
   ```bash
   kasl watch --stop
   ```

2. **Check file permissions**:
   ```bash
   # Linux/macOS
   ls -la ~/.local/share/lacodda/kasl/kasl.db
   
   # Windows
   dir "%LOCALAPPDATA%\lacodda\kasl\kasl.db"
   ```

3. **Fix permissions**:
   ```bash
   # Linux/macOS
   chmod 600 ~/.local/share/lacodda/kasl/kasl.db
   chmod 700 ~/.local/share/lacodda/kasl/
   ```

4. **Check for corruption**:
   ```bash
   sqlite3 ~/.local/share/lacodda/kasl/kasl.db "PRAGMA integrity_check;"
   ```

#### Problem: Migration failures

**Symptoms**:
- "migration failed" errors
- Database schema issues
- Application won't start

**Solutions**:

1. **Check migration status**:
   ```bash
   kasl migrations status
   ```

2. **View migration history**:
   ```bash
   kasl migrations history
   ```

3. **Backup and reset**:
   ```bash
   # Backup current database
   cp ~/.local/share/lacodda/kasl/kasl.db kasl_backup.db
   
   # Remove database (will be recreated)
   rm ~/.local/share/lacodda/kasl/kasl.db
   
   # Restart kasl
   kasl watch
   ```

### Configuration Issues

#### Problem: Configuration not found

**Symptoms**:
- "configuration not found" errors
- Default settings used
- Cannot save configuration

**Solutions**:

1. **Check configuration location**:
   ```bash
   # Linux/macOS
   ls -la ~/.local/share/lacodda/kasl/config.json
   
   # Windows
   dir "%LOCALAPPDATA%\lacodda\kasl\config.json"
   ```

2. **Recreate configuration**:
   ```bash
   kasl init
   ```

3. **Create directory manually**:
   ```bash
   # Linux/macOS
   mkdir -p ~/.local/share/lacodda/kasl
   
   # Windows
   mkdir "%LOCALAPPDATA%\lacodda\kasl"
   ```

#### Problem: Invalid configuration

**Symptoms**:
- "invalid configuration" errors
- Application crashes on startup
- Settings not applied

**Solutions**:

1. **Validate JSON syntax**:
   ```bash
   # Using Python
   python -m json.tool ~/.local/share/lacodda/kasl/config.json
   
   # Using jq
   jq . ~/.local/share/lacodda/kasl/config.json
   ```

2. **Reset configuration**:
   ```bash
   kasl init --delete
   kasl init
   ```

3. **Check for syntax errors**:
   ```bash
   # Common issues:
   # - Missing commas
   # - Extra commas
   # - Unquoted strings
   # - Invalid JSON types
   ```

### API Integration Issues

#### Problem: Authentication failures

**Symptoms**:
- "authentication failed" errors
- Cannot connect to APIs
- Session expired messages

**Solutions**:

1. **Clear cached sessions**:
   ```bash
   # Remove session files
   rm ~/.local/share/lacodda/kasl/.gitlab_session
   rm ~/.local/share/lacodda/kasl/.jira_session
   rm ~/.local/share/lacodda/kasl/.si_session
   ```

2. **Reconfigure integration**:
   ```bash
   kasl init
   ```

3. **Check credentials**:
   - Verify API tokens are valid
   - Check username/password
   - Confirm API URLs

4. **Test connectivity**:
   ```bash
   # Test GitLab
   curl -H "Authorization: Bearer YOUR_TOKEN" https://gitlab.com/api/v4/user
   
   # Test Jira
   curl -u "username:password" https://jira.company.com/rest/api/2/myself
   ```

#### Problem: Network connectivity

**Symptoms**:
- "connection failed" errors
- Timeout errors
- Cannot reach APIs

**Solutions**:

1. **Check network connectivity**:
   ```bash
   # Test basic connectivity
   ping gitlab.com
   ping jira.company.com
   
   # Test HTTPS
   curl -I https://gitlab.com
   ```

2. **Check proxy settings**:
   ```bash
   # Set proxy environment variables
   export HTTP_PROXY=http://proxy.company.com:8080
   export HTTPS_PROXY=http://proxy.company.com:8080
   ```

3. **Check firewall settings**:
   - Ensure outbound HTTPS (443) is allowed
   - Check corporate firewall rules
   - Verify VPN connection if required

### Task Management Issues

#### Problem: Tasks not found

**Symptoms**:
- Empty task lists
- "task not found" errors
- Tasks not saving

**Solutions**:

1. **Check database**:
   ```bash
   sqlite3 ~/.local/share/lacodda/kasl/kasl.db "SELECT * FROM tasks;"
   ```

2. **Verify task creation**:
   ```bash
   # Create test task
   kasl task --name "Test task" --completeness 0
   
   # List tasks
   kasl task --show
   ```

3. **Check for database issues**:
   ```bash
   sqlite3 ~/.local/share/lacodda/kasl/kasl.db "PRAGMA integrity_check;"
   ```

#### Problem: Tag issues

**Symptoms**:
- Tags not saving
- Tag associations lost
- Tag filtering not working

**Solutions**:

1. **Check tag tables**:
   ```bash
   sqlite3 ~/.local/share/lacodda/kasl/kasl.db "SELECT * FROM tags;"
   sqlite3 ~/.local/share/lacodda/kasl/kasl.db "SELECT * FROM task_tags;"
   ```

2. **Recreate tags**:
   ```bash
   kasl tag create --name "test" --color "red"
   kasl tag list
   ```

3. **Check foreign key constraints**:
   ```bash
   sqlite3 ~/.local/share/lacodda/kasl/kasl.db "PRAGMA foreign_keys = ON;"
   ```

### Report Issues

#### Problem: Reports not generating

**Symptoms**:
- Empty reports
- Missing data
- Report generation errors

**Solutions**:

1. **Check workday data**:
   ```bash
   sqlite3 ~/.local/share/lacodda/kasl/kasl.db "SELECT * FROM workdays ORDER BY date DESC LIMIT 5;"
   ```

2. **Check pause data**:
   ```bash
   sqlite3 ~/.local/share/lacodda/kasl/kasl.db "SELECT * FROM pauses ORDER BY start DESC LIMIT 5;"
   ```

3. **Generate report manually**:
   ```bash
   kasl report --last
   ```

#### Problem: Report submission failures

**Symptoms**:
- "report send failed" errors
- Reports not reaching server
- Authentication issues

**Solutions**:

1. **Check API configuration**:
   ```bash
   # Verify SiServer configuration
   cat ~/.local/share/lacodda/kasl/config.json | jq .si
   ```

2. **Test API connectivity**:
   ```bash
   # Test SiServer connection
   curl -X POST https://api.company.com/health
   ```

3. **Check authentication**:
   ```bash
   # Clear session and retry
   rm ~/.local/share/lacodda/kasl/.si_session
   kasl report --send
   ```

## Debug Mode

### Enable Debug Logging

```bash
# Enable debug mode
RUST_LOG=kasl=debug kasl watch --foreground

# Enable trace logging
RUST_LOG=kasl=trace kasl watch --foreground

# Enable SQLite logging
RUST_LOG=kasl=debug kasl report
```

### Debug Information

Debug mode shows:
- Configuration loading
- Database operations
- API requests/responses
- Error details
- Performance metrics

### Common Debug Commands

```bash
# Check configuration
RUST_LOG=kasl=debug kasl init

# Debug task operations
RUST_LOG=kasl=debug kasl task --show

# Debug report generation
RUST_LOG=kasl=debug kasl report

# Debug API operations
RUST_LOG=kasl=debug kasl task --find
```

## Performance Issues

### High CPU Usage

**Symptoms**:
- High CPU usage
- System slowdown
- Battery drain

**Solutions**:

1. **Increase poll interval**:
   ```json
   {
     "monitor": {
       "poll_interval": 1000  // Increase from 500
     }
   }
   ```

2. **Check for multiple instances**:
   ```bash
   ps aux | grep kasl
   kasl watch --stop
   ```

3. **Profile performance**:
   ```bash
   # Linux
   perf record --call-graph=dwarf ./target/release/kasl watch
   perf report
   ```

### High Memory Usage

**Symptoms**:
- High memory consumption
- Memory leaks
- Application crashes

**Solutions**:

1. **Check memory usage**:
   ```bash
   # Linux/macOS
   ps aux | grep kasl
   
   # Windows
   tasklist | findstr kasl
   ```

2. **Restart application**:
   ```bash
   kasl watch --stop
   kasl watch
   ```

3. **Check for memory leaks**:
   ```bash
   # Use valgrind (Linux)
   valgrind --leak-check=full ./target/release/kasl watch
   ```

## Platform-Specific Issues

### Windows Issues

#### Problem: Autostart not working

**Solutions**:
1. **Check Task Scheduler**:
   - Open Task Scheduler
   - Look for kasl tasks
   - Verify task is enabled

2. **Check Registry**:
   ```cmd
   reg query "HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run" /v kasl
   ```

3. **Run as Administrator**:
   ```cmd
   kasl autostart enable
   ```

#### Problem: Permission denied

**Solutions**:
1. **Run as Administrator**:
   - Right-click Command Prompt
   - "Run as administrator"

2. **Check file permissions**:
   ```cmd
   icacls "%LOCALAPPDATA%\lacodda\kasl"
   ```

### macOS Issues

#### Problem: Input monitoring permissions

**Solutions**:
1. **Grant Accessibility permissions**:
   - System Preferences → Security & Privacy → Privacy → Accessibility
   - Add kasl to the list

2. **Grant Input Monitoring permissions**:
   - System Preferences → Security & Privacy → Privacy → Input Monitoring
   - Add kasl to the list

#### Problem: Autostart not working

**Solutions**:
1. **Check LaunchAgents**:
   ```bash
   ls -la ~/Library/LaunchAgents/
   ```

2. **Load LaunchAgent manually**:
   ```bash
   launchctl load ~/Library/LaunchAgents/com.lacodda.kasl.plist
   ```

### Linux Issues

#### Problem: Input device access

**Solutions**:
1. **Check user groups**:
   ```bash
   groups $USER
   ```

2. **Add user to input group**:
   ```bash
   sudo usermod -a -G input $USER
   ```

3. **Check device permissions**:
   ```bash
   ls -la /dev/input/
   ```

#### Problem: systemd service issues

**Solutions**:
1. **Check service status**:
   ```bash
   systemctl --user status kasl
   ```

2. **Enable service**:
   ```bash
   systemctl --user enable kasl
   systemctl --user start kasl
   ```

## Getting Help

### Before Asking for Help

1. **Check this guide** for your specific issue
2. **Enable debug logging** and check output
3. **Try the solutions** provided above
4. **Gather information** about your system

### Information to Provide

When reporting issues, include:
- Operating system and version
- kasl version (`kasl --version`)
- Error messages (with debug logging)
- Steps to reproduce
- System configuration

### Support Channels

- **GitHub Issues**: [https://github.com/lacodda/kasl/issues](https://github.com/lacodda/kasl/issues)
- **Email**: lahtachev@gmail.com
- **Documentation**: [https://kasl.lacodda.com](https://kasl.lacodda.com)
