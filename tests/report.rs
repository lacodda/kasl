#[cfg(test)]
mod tests {
    use kasl::db::{breaks::Breaks, workdays::Workdays};
    use kasl::libs::view::View;
    use chrono::{NaiveDate, NaiveDateTime};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    /// Test context for report command tests.
    struct ReportTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for ReportTestContext {
        /// Sets up a temporary directory for testing database operations.
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            ReportTestContext { _temp_dir: temp_dir }
        }
    }

    /// Tests report generation with breaks.
    ///
    /// Simulates a workday with two breaks and verifies that the report is generated correctly.
    #[test_context(ReportTestContext)]
    #[test]
    fn test_report_with_breaks(_ctx: &mut ReportTestContext) {
        let date = NaiveDate::from_ymd_opt(2025, 6, 24).unwrap();
        let start_time = NaiveDateTime::parse_from_str("2025-06-24 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end_time = NaiveDateTime::parse_from_str("2025-06-24 17:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

        let mut workdays = Workdays::new().unwrap();
        workdays.insert_start(date).unwrap();
        workdays.insert_end(date).unwrap();

        // Insert two breaks: 10:00-10:30 and 12:00-13:00.
        let mut breaks = Breaks::new().unwrap();
        breaks
            .conn
            .lock()
            .execute(
                "INSERT INTO breaks (start, end, duration) VALUES (?, ?, ?)",
                ["2025-06-24 10:00:00", "2025-06-24 10:30:00", &(30 * 60).to_string()],
            )
            .unwrap();
        breaks
            .conn
            .lock()
            .execute(
                "INSERT INTO breaks (start, end, duration) VALUES (?, ?, ?)",
                ["2025-06-24 12:00:00", "2025-06-24 13:00:00", &(60 * 60).to_string()],
            )
            .unwrap();

        let workday = workdays.fetch(date).unwrap().unwrap();
        let breaks_vec = breaks.fetch(date, 20).unwrap();
        let tasks = vec![];

        let output = View::report(&workday, &breaks_vec, &tasks);
        assert!(output.is_ok());
    }

    /// Tests report generation without breaks.
    ///
    /// Simulates a workday without breaks and verifies that the report is generated correctly.
    #[test_context(ReportTestContext)]
    #[test]
    fn test_report_no_breaks(_ctx: &mut ReportTestContext) {
        let date = NaiveDate::from_ymd_opt(2025, 6, 24).unwrap();
        let start_time = NaiveDateTime::parse_from_str("2025-06-24 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let end_time = NaiveDateTime::parse_from_str("2025-06-24 17:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

        let mut workdays = Workdays::new().unwrap();
        workdays.insert_start(date).unwrap();
        workdays.insert_end(date).unwrap();

        let breaks = Breaks::new().unwrap();
        let workday = workdays.fetch(date).unwrap().unwrap();
        let breaks_vec = breaks.fetch(date, 20).unwrap();
        let tasks = vec![];

        let output = View::report(&workday, &breaks_vec, &tasks);
        assert!(output.is_ok());
    }
}
