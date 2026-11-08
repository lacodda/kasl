#[cfg(test)]
mod tests {
    use kasl::libs::config::schema::SCHEMA_URL;
    use kasl::libs::config::{Config, MonitorConfig, ProductivityConfig};
    use serial_test::serial;
    use std::path::Path;
    use std::process::{Command, Output, Stdio};
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    /// Test context to ensure a clean environment for each config test.
    /// It sets up a temporary directory to act as the user's home/appdata directory.
    struct ConfigTestContext {
        _temp_dir: TempDir,
        min_pause_duration: u64,
        pause_threshold: u64,
        poll_interval: u64,
        activity_threshold: u64,
        min_work_interval: u64,
    }

    impl TestContext for ConfigTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            // Mock the home/appdata directory for cross-platform compatibility.
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("HOME", temp_dir.path());
            }
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("LOCALAPPDATA", temp_dir.path());
            }
            ConfigTestContext {
                _temp_dir: temp_dir,
                min_pause_duration: 20,
                pause_threshold: 60,
                poll_interval: 500,
                activity_threshold: 30,
                min_work_interval: 5,
            }
        }
    }

    #[test_context(ConfigTestContext)]
    #[serial]
    #[test]
    fn test_default_config(_ctx: &mut ConfigTestContext) {
        let config = Config::default();
        assert!(config.monitor.is_none());
        assert!(config.si.is_none());
        assert!(config.jira.is_none());
    }

    #[test_context(ConfigTestContext)]
    #[serial]
    #[test]
    fn test_read_nonexistent_config(_ctx: &mut ConfigTestContext) {
        // When no config file exists, read() should return the default config.
        let config = Config::read().unwrap();
        assert_eq!(config.monitor, None);
    }

    #[test_context(ConfigTestContext)]
    #[serial]
    #[test]
    fn test_save_and_read_config(ctx: &mut ConfigTestContext) {
        let config = Config {
            monitor: Some(MonitorConfig {
                min_pause_duration: ctx.min_pause_duration,
                pause_threshold: ctx.pause_threshold,
                poll_interval: ctx.poll_interval,
                activity_threshold: ctx.activity_threshold,
                min_work_interval: ctx.min_work_interval,
                ..Default::default()
            }),
            ..Default::default()
        };
        config.save().unwrap();
        let monitor_config = Config::read().unwrap().monitor.unwrap();

        assert_eq!(monitor_config.min_pause_duration, ctx.min_pause_duration);
        assert_eq!(monitor_config.pause_threshold, ctx.pause_threshold);
        assert_eq!(monitor_config.poll_interval, ctx.poll_interval);
        assert_eq!(monitor_config.activity_threshold, ctx.activity_threshold);
        assert_eq!(monitor_config.min_work_interval, ctx.min_work_interval);
    }

    #[test_context(ConfigTestContext)]
    #[serial]
    #[test]
    fn a_saved_file_names_its_schema_first(_ctx: &mut ConfigTestContext) {
        Config::default().save().unwrap();
        let text = std::fs::read_to_string(Config::path().unwrap()).unwrap();
        let first_key = text.lines().nth(1).unwrap_or_default().trim();
        assert_eq!(
            first_key,
            format!("\"$schema\": \"{SCHEMA_URL}\""),
            "an editor finds the schema in the file itself:\n{text}"
        );
    }

    #[test_context(ConfigTestContext)]
    #[serial]
    #[test]
    fn a_retired_block_is_read_past_and_dropped_on_save(_ctx: &mut ConfigTestContext) {
        // `server` held a URL and a plain-text token that nothing read; a
        // file that still has it must keep working, and lose it on save.
        std::fs::write(
            Config::path().unwrap(),
            r#"{ "server": { "api_url": "https://reports.example.com", "auth_token": "secret" }, "monitor": { "pause_threshold": 90 } }"#,
        )
        .unwrap();
        let config = Config::read().unwrap();
        assert_eq!(config.monitor.as_ref().unwrap().pause_threshold, 90);
        assert_eq!(
            config.monitor.as_ref().unwrap().min_pause_duration,
            20,
            "a partial block takes the rest from its defaults"
        );
        config.save().unwrap();
        let text = std::fs::read_to_string(Config::path().unwrap()).unwrap();
        assert!(!text.contains("auth_token"), "the retired token is gone from the file:\n{text}");
    }

    #[test_context(ConfigTestContext)]
    #[serial]
    #[test]
    fn test_default_monitor_config(ctx: &mut ConfigTestContext) {
        let monitor_config = MonitorConfig::default();
        assert_eq!(monitor_config.min_pause_duration, ctx.min_pause_duration);
        assert_eq!(monitor_config.pause_threshold, ctx.pause_threshold);
        assert_eq!(monitor_config.poll_interval, ctx.poll_interval);
        assert_eq!(monitor_config.activity_threshold, ctx.activity_threshold);
    }

    #[serial]
    #[test]
    fn test_default_productivity_config() {
        let productivity_config = ProductivityConfig::default();
        assert_eq!(productivity_config.min_productivity_threshold, 75.0);
        assert_eq!(productivity_config.workday_hours, 8.0);
        assert_eq!(productivity_config.min_workday_fraction_before_suggest, 0.5);
    }

    /// `kasl` run against a data directory of its own, with no terminal.
    fn kasl(dir: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kasl"))
            .args(args)
            .env("HOME", dir)
            .env("LOCALAPPDATA", dir)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn text(out: &Output) -> String {
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    }

    fn config_file(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("lacodda").join("kasl").join("config.json")).unwrap()
    }

    #[serial]
    #[test]
    fn set_writes_only_the_setting_it_was_given() {
        let dir = TempDir::new().unwrap();
        let out = kasl(dir.path(), &["config", "set", "monitor.pause_threshold", "90"]);
        assert!(out.status.success(), "{}", text(&out));

        let written: serde_json::Value = serde_json::from_str(&config_file(dir.path())).unwrap();
        assert_eq!(
            written,
            serde_json::json!({ "$schema": SCHEMA_URL, "monitor": { "pause_threshold": 90 } }),
            "defaults stay defaults: written out, they would stop following kasl's"
        );

        let out = kasl(dir.path(), &["config", "get", "monitor.pause_threshold"]);
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "90");
        let out = kasl(dir.path(), &["config", "get", "monitor.min_pause_duration", "--json"]);
        let got: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            got,
            serde_json::json!({ "key": "monitor.min_pause_duration", "value": 20, "source": "default" })
        );
    }

    #[serial]
    #[test]
    fn a_refused_set_leaves_the_file_as_it_was() {
        let dir = TempDir::new().unwrap();
        assert!(kasl(dir.path(), &["config", "set", "report.language", "ru"]).status.success());
        let before = config_file(dir.path());

        for args in [
            ["config", "set", "monitor.pause_treshold", "90"],
            ["config", "set", "inbox.poll_interval_secs", "5"],
            ["config", "set", "report.language", "de"],
            ["config", "set", "kasl_server.pulse", "true"],
        ] {
            let out = kasl(dir.path(), &args);
            assert!(!out.status.success(), "`kasl {}` should be refused:\n{}", args.join(" "), text(&out));
        }
        assert_eq!(config_file(dir.path()), before);
    }

    #[serial]
    #[test]
    fn unset_takes_a_setting_back_to_its_default() {
        let dir = TempDir::new().unwrap();
        assert!(kasl(dir.path(), &["config", "set", "monitor.pause_threshold", "90"]).status.success());
        let out = kasl(dir.path(), &["config", "unset", "monitor.pause_threshold"]);
        assert!(out.status.success(), "{}", text(&out));
        assert!(text(&out).contains("60"), "says the default it is back to:\n{}", text(&out));
        let out = kasl(dir.path(), &["config", "get", "monitor.pause_threshold", "--json"]);
        let got: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(got["source"], "default");
    }

    #[serial]
    #[test]
    fn check_fails_on_a_problem_and_names_it() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("lacodda").join("kasl")).unwrap();
        std::fs::write(
            dir.path().join("lacodda").join("kasl").join("config.json"),
            r#"{ "monitr": { "pause_threshold": 90 } }"#,
        )
        .unwrap();

        let out = kasl(dir.path(), &["config", "check", "--json"]);
        assert!(!out.status.success(), "a problem fails the check");
        let found: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(found["problems"][0]["key"], "monitr");
        assert!(found["problems"][0]["message"].as_str().unwrap().contains("`monitor`"), "{found}");

        std::fs::write(
            dir.path().join("lacodda").join("kasl").join("config.json"),
            r#"{ "monitor": { "pause_threshold": 90 } }"#,
        )
        .unwrap();
        let out = kasl(dir.path(), &["config", "check"]);
        assert!(out.status.success(), "{}", text(&out));
    }

    #[serial]
    #[test]
    fn setup_delete_removes_the_file_and_nothing_else() {
        let dir = TempDir::new().unwrap();
        assert!(kasl(dir.path(), &["config", "set", "monitor.pause_threshold", "90"]).status.success());
        let out = kasl(dir.path(), &["setup", "--delete"]);
        assert!(out.status.success(), "{}", text(&out));
        assert!(!dir.path().join("lacodda").join("kasl").join("config.json").exists(), "{}", text(&out));
    }
}
