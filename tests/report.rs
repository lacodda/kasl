#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use kasl::db::{pauses::Pauses, workdays::Workdays};
    use kasl::libs::view::View;
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

    /// Tests report generation with pauses.
    ///
    /// Simulates a workday with two pauses and verifies that the report is generated correctly.
    #[test_context(ReportTestContext)]
    #[test]
    fn test_report_with_pauses(_ctx: &mut ReportTestContext) {
        let date = NaiveDate::from_ymd_opt(2025, 6, 24).unwrap();

        // Setup workday
        let mut workdays = Workdays::new().unwrap();
        workdays.insert_start(date).unwrap();

        // Manually update start/end times for deterministic test
        workdays
            .conn
            .execute(
                "UPDATE workdays SET start = '2025-06-24 09:00:00', end = '2025-06-24 17:00:00' WHERE date = ?",
                [&date.to_string()],
            )
            .unwrap();

        // Insert two pauses: 10:00-10:30 and 12:00-13:00.
        let pauses_db = Pauses::new().unwrap();
        let conn = pauses_db.conn.lock();
        conn.execute(
            "INSERT INTO pauses (start, end, duration) VALUES ('2025-06-24 10:00:00', '2025-06-24 10:30:00', ?)",
            [(30 * 60).to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO pauses (start, end, duration) VALUES ('2025-06-24 12:00:00', '2025-06-24 13:00:00', ?)",
            [(60 * 60).to_string()],
        )
        .unwrap();

        // Drop lock explicitly
        drop(conn);

        let workday = workdays.fetch(date).unwrap().unwrap();
        let pauses_vec = pauses_db.fetch(date, 0).unwrap(); // min_duration = 0 to fetch all
        let tasks = vec![];

        // Pass the same pauses for both long_breaks and all_pauses in tests
        let output = View::report(&workday, &pauses_vec, &pauses_vec, &tasks);
        assert!(output.is_ok());
    }

    /// Tests report generation without pauses.
    ///
    /// Simulates a workday without pauses and verifies that the report is generated correctly.
    #[test_context(ReportTestContext)]
    #[test]
    fn test_report_no_pauses(_ctx: &mut ReportTestContext) {
        let date = NaiveDate::from_ymd_opt(2025, 6, 25).unwrap();

        let mut workdays = Workdays::new().unwrap();
        workdays.insert_start(date).unwrap();
        workdays
            .conn
            .execute(
                "UPDATE workdays SET start = '2025-06-25 09:00:00', end = '2025-06-25 17:00:00' WHERE date = ?",
                [&date.to_string()],
            )
            .unwrap();

        let pauses_db = Pauses::new().unwrap();

        let workday = workdays.fetch(date).unwrap().unwrap();
        let pauses_vec = pauses_db.fetch(date, 0).unwrap();
        let tasks = vec![];

        assert_eq!(pauses_vec.len(), 0);
        // Pass empty pauses for both parameters
        let output = View::report(&workday, &pauses_vec, &pauses_vec, &tasks);
        assert!(output.is_ok());
    }
}
