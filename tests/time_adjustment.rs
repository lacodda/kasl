#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use kasl::db::workdays::Workdays;
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    struct AdjustTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for AdjustTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            AdjustTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(AdjustTestContext)]
    #[test]
    fn test_adjust_start_time(_ctx: &mut AdjustTestContext) {
        let mut workdays = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

        // Create workday with specific start time
        workdays.insert_start(date).unwrap();
        workdays
            .conn
            .execute("UPDATE workdays SET start = '2025-01-15 09:00:00' WHERE date = ?", [&date.to_string()])
            .unwrap();

        // Adjust start time by 30 minutes
        let new_start = date.and_hms_opt(9, 30, 0).unwrap();
        workdays.update_start(date, new_start).unwrap();

        // Verify update
        let updated = workdays.fetch(date).unwrap().unwrap();
        assert_eq!(updated.start, new_start);
    }

    #[test_context(AdjustTestContext)]
    #[test]
    fn test_adjust_end_time(_ctx: &mut AdjustTestContext) {
        let mut workdays = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();

        // Create workday with end time
        workdays.insert_start(date).unwrap();
        workdays.insert_end(date).unwrap();
        workdays
            .conn
            .execute(
                "UPDATE workdays SET start = '2025-01-15 09:00:00', end = '2025-01-15 17:00:00' WHERE date = ?",
                [&date.to_string()],
            )
            .unwrap();

        // Adjust end time earlier by 1 hour
        let new_end = date.and_hms_opt(16, 0, 0).unwrap();
        workdays.update_end(date, Some(new_end)).unwrap();

        // Verify update
        let updated = workdays.fetch(date).unwrap().unwrap();
        assert_eq!(updated.end, Some(new_end));
    }
}
