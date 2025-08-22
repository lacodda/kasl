#[cfg(test)]
mod tests {
    use kasl::libs::config::{Config, MonitorConfig, ServerConfig, ProductivityConfig};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    /// Test context to ensure a clean environment for each config test.
    /// It sets up a temporary directory to act as the user's home/appdata directory.
    struct ConfigTestContext {
        _temp_dir: TempDir,
        min_pause_duration: u64,
        pause_threshold: u64,
        poll_interval: u64,
        activity_threshold: u64,
        min_work_interval: u64,
        api_url: String,
        auth_token: String,
    }

    impl TestContext for ConfigTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            // Mock the home/appdata directory for cross-platform compatibility.
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            ConfigTestContext {
                _temp_dir: temp_dir,
                min_pause_duration: 20,
                pause_threshold: 60,
                poll_interval: 500,
                activity_threshold: 30,
                min_work_interval: 5,
                api_url: "https://api.example.com".to_string(),
                auth_token: "token123".to_string(),
            }
        }
    }

    #[test_context(ConfigTestContext)]
    #[test]
    fn test_default_config(_ctx: &mut ConfigTestContext) {
        let config = Config::default();
        assert!(config.monitor.is_none());
        assert!(config.server.is_none());
        assert!(config.si.is_none());
        assert!(config.gitlab.is_none());
        assert!(config.jira.is_none());
    }

    #[test_context(ConfigTestContext)]
    #[test]
    fn test_read_nonexistent_config(_ctx: &mut ConfigTestContext) {
        // When no config file exists, read() should return the default config.
        let config = Config::read().unwrap();
        assert_eq!(config.monitor, None);
        assert_eq!(config.server, None);
    }

    #[test_context(ConfigTestContext)]
    #[test]
    fn test_save_and_read_config(ctx: &mut ConfigTestContext) {
        let config = Config {
            monitor: Some(MonitorConfig {
                min_pause_duration: ctx.min_pause_duration,
                pause_threshold: ctx.pause_threshold,
                poll_interval: ctx.poll_interval,
                activity_threshold: ctx.activity_threshold,
                min_work_interval: ctx.min_work_interval,
            }),
            server: Some(ServerConfig {
                api_url: ctx.api_url.clone(),
                auth_token: ctx.auth_token.clone(),
            }),
            si: None,
            gitlab: None,
            jira: None,
            productivity: None,
        };
        config.save().unwrap();
        let read_config = Config::read().unwrap();
        let monitor_config = read_config.monitor.unwrap();
        let server_config = read_config.server.unwrap();

        assert_eq!(monitor_config.min_pause_duration, ctx.min_pause_duration);
        assert_eq!(monitor_config.pause_threshold, ctx.pause_threshold);
        assert_eq!(monitor_config.poll_interval, ctx.poll_interval);
        assert_eq!(monitor_config.activity_threshold, ctx.activity_threshold);
        assert_eq!(monitor_config.min_work_interval, ctx.min_work_interval);
        assert_eq!(server_config.api_url, ctx.api_url.clone());
        assert_eq!(server_config.auth_token, ctx.auth_token.clone());
    }

    #[test_context(ConfigTestContext)]
    #[test]
    fn test_default_monitor_config(ctx: &mut ConfigTestContext) {
        let monitor_config = MonitorConfig::default();
        assert_eq!(monitor_config.min_pause_duration, ctx.min_pause_duration);
        assert_eq!(monitor_config.pause_threshold, ctx.pause_threshold);
        assert_eq!(monitor_config.poll_interval, ctx.poll_interval);
        assert_eq!(monitor_config.activity_threshold, ctx.activity_threshold);
    }

    #[test]
    fn test_default_productivity_config() {
        let productivity_config = ProductivityConfig::default();
        assert_eq!(productivity_config.min_productivity_threshold, 75.0);
        assert_eq!(productivity_config.workday_hours, 8.0);
        assert_eq!(productivity_config.min_workday_fraction_before_suggest, 0.5);
        assert_eq!(productivity_config.min_break_duration, 20);
        assert_eq!(productivity_config.max_break_duration, 180);
    }
}
