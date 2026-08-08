#[cfg(test)]
mod tests {
    use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
    use kasl::db::workdays::Workday;
    use kasl::libs::pause::Pause;
    use kasl::libs::productivity::Productivity;
    use serial_test::serial;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    /// Test context for productivity calculation tests.
    struct ProductivityTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for ProductivityTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("HOME", temp_dir.path());
            }
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("LOCALAPPDATA", temp_dir.path());
            }
            ProductivityTestContext { _temp_dir: temp_dir }
        }
    }

    fn create_test_workday() -> Workday {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let start = NaiveDateTime::new(date, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        let end = NaiveDateTime::new(date, NaiveTime::from_hms_opt(17, 0, 0).unwrap());

        Workday {
            id: 1,
            date,
            start,
            end: Some(end),
        }
    }

    fn create_test_workday_with_duration(duration: Duration) -> Workday {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let start = NaiveDateTime::new(date, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        let end = start + duration;

        Workday {
            id: 1,
            date,
            start,
            end: Some(end),
        }
    }

    #[test_context(ProductivityTestContext)]
    #[serial]
    #[test]
    fn test_productivity_calculation_no_pauses(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();
        let pauses = vec![];

        let productivity_calc = Productivity::with_test_data(&workday, pauses, vec![]);
        let productivity = productivity_calc.calculate_productivity();
        assert_eq!(productivity, 100.0);
    }

    #[test_context(ProductivityTestContext)]
    #[serial]
    #[test]
    fn test_productivity_calculation_with_pauses(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();
        let pauses = vec![Pause::detected(
            1,
            NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
            Some(NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(10, 15, 0).unwrap())),
            Some(Duration::minutes(15)),
        )];

        let productivity_calc = Productivity::with_test_data(&workday, pauses, vec![]);
        let productivity = productivity_calc.calculate_productivity();
        // 8 hours work, 15 minutes pause = 7:45 / 8:00 = 96.875%
        assert!((productivity - 96.875).abs() < 0.001);
    }

    #[test_context(ProductivityTestContext)]
    #[serial]
    #[test]
    fn test_productivity_calculation_with_breaks(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();
        // A manual break is now just a protected long pause.
        let long_pauses = vec![Pause {
            id: 1,
            start: NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(12, 0, 0).unwrap()),
            end: Some(NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(13, 0, 0).unwrap())),
            duration: Some(Duration::hours(1)),
            protected: true,
        }];

        let productivity_calc = Productivity::with_test_data(&workday, vec![], long_pauses);
        let productivity = productivity_calc.calculate_productivity();
        // 8 hours total, 1 hour break excluded = 7/7 = 100%
        assert_eq!(productivity, 100.0);
    }

    #[test_context(ProductivityTestContext)]
    #[serial]
    #[test]
    fn test_productivity_calculation_with_pauses_and_breaks(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();
        let short_pauses = vec![Pause::detected(
            1,
            NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
            Some(NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(10, 15, 0).unwrap())),
            Some(Duration::minutes(15)),
        )];
        // A manual break is now just a protected long pause.
        let long_pauses = vec![Pause {
            id: 1,
            start: NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(12, 0, 0).unwrap()),
            end: Some(NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(13, 0, 0).unwrap())),
            duration: Some(Duration::hours(1)),
            protected: true,
        }];

        let productivity_calc = Productivity::with_test_data(&workday, short_pauses, long_pauses);
        let productivity = productivity_calc.calculate_productivity();
        // Available = 8h - 1h long pause = 7h; Net = 7h - 15min short pause = 6h45m
        // Productivity = 405 / 420 * 100 = 96.42857...%
        assert!((productivity - 96.428571).abs() < 0.001);
    }

    #[test_context(ProductivityTestContext)]
    #[serial]
    #[test]
    fn test_productivity_for_intervals_no_pauses(_ctx: &mut ProductivityTestContext) {
        let work_time = Duration::hours(6);
        let workday = create_test_workday_with_duration(work_time);
        let pauses = vec![];

        let productivity_calc = Productivity::with_test_data(&workday, pauses, vec![]);
        let productivity = productivity_calc.calculate_productivity();
        assert_eq!(productivity, 100.0);
    }

    #[test_context(ProductivityTestContext)]
    #[serial]
    #[test]
    fn test_productivity_for_intervals_with_pauses(_ctx: &mut ProductivityTestContext) {
        let work_time = Duration::hours(6);
        let workday = create_test_workday_with_duration(work_time);
        let pauses = vec![Pause::detected(
            1,
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap().and_hms_opt(10, 0, 0).unwrap(),
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap().and_hms_opt(10, 15, 0).unwrap()),
            Some(Duration::minutes(15)),
        )];

        let productivity_calc = Productivity::with_test_data(&workday, pauses, vec![]);
        let productivity = productivity_calc.calculate_productivity();
        // 6 hours work, 15 minutes pause = 5:45 / 6:00 = 95.833%
        assert!((productivity - 95.833333).abs() < 0.001);
    }

    #[test_context(ProductivityTestContext)]
    #[serial]
    #[test]
    fn test_productivity_edge_cases(_ctx: &mut ProductivityTestContext) {
        // Test zero work time
        let zero_work = Duration::zero();
        let workday = create_test_workday_with_duration(zero_work);
        let productivity_calc = Productivity::with_test_data(&workday, vec![], vec![]);
        let productivity = productivity_calc.calculate_productivity();
        assert_eq!(productivity, 0.0);

        // Test productivity clamping (should not exceed 100%)
        let work_time = Duration::hours(1);
        let workday = create_test_workday_with_duration(work_time);
        let negative_pause = vec![Pause::detected(
            1,
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap().and_hms_opt(10, 0, 0).unwrap(),
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap().and_hms_opt(9, 0, 0).unwrap()), // End before start (invalid)
            Some(Duration::hours(-2)),                                                         // Negative duration
        )];

        let productivity_calc = Productivity::with_test_data(&workday, vec![], negative_pause);
        let productivity = productivity_calc.calculate_productivity();
        assert!((0.0..=100.0).contains(&productivity));
    }

    #[test_context(ProductivityTestContext)]
    #[serial]
    #[test]
    fn test_productivity_boundary_values(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();

        // Test with pauses equal to work time (should be 0% productivity)
        let massive_pause = vec![Pause::detected(1, workday.start, workday.end, Some(Duration::hours(8)))];

        let productivity_calc = Productivity::with_test_data(&workday, vec![], massive_pause);
        let productivity = productivity_calc.calculate_productivity();
        assert_eq!(productivity, 0.0);

        // Test with a manual break (now a protected long pause) equal to work time
        // (should be 0% productivity due to no available time)
        let massive_break = vec![Pause {
            id: 1,
            start: workday.start,
            end: workday.end,
            duration: Some(Duration::hours(8)),
            protected: true,
        }];

        let productivity_calc = Productivity::with_test_data(&workday, vec![], massive_break);
        let productivity = productivity_calc.calculate_productivity();
        assert_eq!(productivity, 0.0);
    }
}
