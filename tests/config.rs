#[cfg(test)]
mod tests {
    use kasl::libs::config::{Config, MonitorConfig, ServerConfig};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    struct ConfigTestContext {
        _temp_dir: TempDir,
        min_break_duration: u64,
        break_threshold: u64,
        poll_interval: u64,
        api_url: String,
        auth_token: String,
    }

    impl TestContext for ConfigTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            ConfigTestContext {
                _temp_dir: temp_dir,
                min_break_duration: 20,
                break_threshold: 60,
                poll_interval: 500,
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
    }

    #[test_context(ConfigTestContext)]
    #[test]
    fn test_read_nonexistent_config(_ctx: &mut ConfigTestContext) {
        let config = Config::read().unwrap();
        assert_eq!(config.monitor, None);
        assert_eq!(config.server, None);
    }

    #[test_context(ConfigTestContext)]
    #[test]
    fn test_save_and_read_config(ctx: &mut ConfigTestContext) {
        let config = Config {
            monitor: Some(MonitorConfig {
                min_break_duration: ctx.min_break_duration,
                break_threshold: ctx.break_threshold,
                poll_interval: ctx.poll_interval,
            }),
            server: Some(ServerConfig {
                api_url: ctx.api_url.clone(),
                auth_token: ctx.auth_token.clone(),
            }),
            si: None,
            gitlab: None,
            jira: None,
        };

        config.save().unwrap();
        let read_config = Config::read().unwrap();
        let monitor_config = read_config.monitor.unwrap();
        let server_config = read_config.server.unwrap();

        assert_eq!(monitor_config.min_break_duration, ctx.min_break_duration);
        assert_eq!(monitor_config.break_threshold, ctx.break_threshold);
        assert_eq!(monitor_config.poll_interval, ctx.poll_interval);
        assert_eq!(server_config.api_url, ctx.api_url.clone());
        assert_eq!(server_config.auth_token, ctx.auth_token.clone());
    }

    #[test_context(ConfigTestContext)]
    #[test]
    fn test_default_monitor_config(ctx: &mut ConfigTestContext) {
        let monitor_config = MonitorConfig::default();
        assert_eq!(monitor_config.min_break_duration, ctx.min_break_duration);
        assert_eq!(monitor_config.break_threshold, ctx.break_threshold);
        assert_eq!(monitor_config.poll_interval, ctx.poll_interval);
    }
}
