#[cfg(test)]
mod tests {
    use chrono::Local;
    use kasl::db::workdays::Workdays;
    use kasl::libs::config::MonitorConfig;
    use kasl::libs::monitor::Monitor;
    use std::error::Error;
    use tempfile::TempDir;
    use test_context::{test_context, AsyncTestContext};
    use tokio::time::{self, Duration, Instant};

    struct MonitorTestContext {
        _temp_dir: TempDir,
    }

    impl AsyncTestContext for MonitorTestContext {
        async fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            MonitorTestContext { _temp_dir: temp_dir }
        }
    }

    async fn run_monitor_test(monitor: &mut Monitor) -> Result<(), Box<dyn Error>> {
        let activity_detected = monitor.detect_activity();
        let today = Local::now().date_naive();

        if activity_detected {
            let activity_duration = monitor.activity_start.lock().unwrap();
            if let Some(start) = *activity_duration {
                if start.elapsed() >= Duration::from_secs(monitor.config.activity_threshold as u64) {
                    if monitor.workdays.fetch(today)?.is_none() {
                        monitor.workdays.insert_start(today)?;
                    }
                    *monitor.activity_start.lock().unwrap() = None;
                }
            }
        }
        Ok(())
    }

    #[test_context(MonitorTestContext)]
    #[tokio::test]
    async fn test_workday_start_detection(_ctx: &mut MonitorTestContext) {
        let config = MonitorConfig {
            min_break_duration: 20,
            break_threshold: 60,
            poll_interval: 500,
            activity_threshold: 1, // Reduced for testing
        };
        let mut monitor = Monitor::new(config).unwrap();
        let today = Local::now().date_naive();

        let mut workdays = Workdays::new().unwrap();
        assert!(workdays.fetch(today).unwrap().is_none());

        // Simulate 2 seconds of activity
        {
            let mut last_activity = monitor.last_activity.lock().unwrap();
            let mut activity_start = monitor.activity_start.lock().unwrap();
            *last_activity = Instant::now();
            *activity_start = Some(Instant::now());
        }
        time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Run monitor logic for one iteration
        run_monitor_test(&mut monitor).await.unwrap();

        let workday = workdays.fetch(today).unwrap();
        assert!(workday.is_some());
        assert_eq!(workday.unwrap().date, today);
    }

    #[test_context(MonitorTestContext)]
    #[tokio::test]
    async fn test_no_workday_on_short_activity(_ctx: &mut MonitorTestContext) {
        let config = MonitorConfig {
            min_break_duration: 20,
            break_threshold: 60,
            poll_interval: 500,
            activity_threshold: 30,
        };
        let mut monitor = Monitor::new(config).unwrap();
        let today = Local::now().date_naive();

        let mut workdays = Workdays::new().unwrap();
        assert!(workdays.fetch(today).unwrap().is_none());

        // Simulate short activity (less than threshold)
        {
            let mut last_activity = monitor.last_activity.lock().unwrap();
            let mut activity_start = monitor.activity_start.lock().unwrap();
            *last_activity = Instant::now();
            *activity_start = Some(Instant::now());
        }
        time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Run monitor logic for one iteration
        run_monitor_test(&mut monitor).await.unwrap();

        let workday = workdays.fetch(today).unwrap();
        assert!(workday.is_none());
    }
}
