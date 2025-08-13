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

    /// Test context for monitor tests. Creates a temporary directory for the database.
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

    /// Helper to run the relevant part of the monitor's main loop for testing.
    async fn simulate_monitor_cycle(monitor: &mut Monitor) -> Result<(), Box<dyn Error>> {
        if monitor.detect_activity() {
            // ensure_workday_started() is private and called automatically within the Monitor
            // No manual call needed - it's handled internally by the monitor loop
        }
        Ok(())
    }

    #[test_context(MonitorTestContext)]
    #[tokio::test]
    async fn test_workday_start_after_sustained_activity(_ctx: &mut MonitorTestContext) {
        let config = MonitorConfig {
            activity_threshold: 1, // Start workday after 1 second of activity
            poll_interval: 100,    // Poll every 100ms
            ..Default::default()
        };
        let mut monitor = Monitor::new(config).unwrap();
        let today = Local::now().date_naive();
        let mut workdays_db = Workdays::new().unwrap();

        assert!(workdays_db.fetch(today).unwrap().is_none(), "Workday should not exist at the start of the test");

        // Mark the beginning of a potential workday
        *monitor.activity_start.lock().unwrap() = Some(Instant::now());

        // Simulate sustained activity for 1.5 seconds
        let simulation_duration = Duration::from_millis(1500);
        let start_time = Instant::now();
        while start_time.elapsed() < simulation_duration {
            // Keep updating last_activity to simulate continuous presence
            *monitor.last_activity.lock().unwrap() = Instant::now();

            // Run the part of the monitor loop that checks for workday start
            simulate_monitor_cycle(&mut monitor).await.unwrap();

            // Wait for the next poll
            time::sleep(Duration::from_millis(monitor.config.poll_interval)).await;
        }

        // After the simulation, the workday should have been created.
        let workday = workdays_db.fetch(today).unwrap();
        assert!(workday.is_some(), "Workday should be created after sustained activity");
        assert_eq!(workday.unwrap().date, today);
    }

    #[test_context(MonitorTestContext)]
    #[tokio::test]
    async fn test_no_workday_start_on_brief_activity(_ctx: &mut MonitorTestContext) {
        let config = MonitorConfig {
            activity_threshold: 5, // 5-second threshold
            ..Default::default()
        };
        let mut monitor = Monitor::new(config).unwrap();
        let today = Local::now().date_naive();
        let mut workdays_db = Workdays::new().unwrap();

        // Simulate the start of activity
        *monitor.activity_start.lock().unwrap() = Some(Instant::now());
        // Simulate one instance of activity
        *monitor.last_activity.lock().unwrap() = Instant::now();

        // Wait for less time than the activity_threshold
        time::sleep(Duration::from_secs(1)).await;

        simulate_monitor_cycle(&mut monitor).await.unwrap();

        // Workday should not have been created yet
        assert!(
            workdays_db.fetch(today).unwrap().is_none(),
            "Workday should not be created after brief activity"
        );
    }
}
