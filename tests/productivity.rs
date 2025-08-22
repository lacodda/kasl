#[cfg(test)]
mod tests {
    use kasl::libs::productivity;
    use kasl::libs::pause::Pause;
    use kasl::db::workdays::Workday;
    use kasl::db::breaks::Break;
    use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    /// Test context for productivity calculation tests.
    struct ProductivityTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for ProductivityTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
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

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_calculation_no_pauses(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();
        let pauses = vec![];
        let breaks = vec![];
        
        let productivity = productivity::calculate_productivity(&workday, &pauses, &breaks);
        assert_eq!(productivity, 100.0);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_calculation_with_pauses(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();
        let pauses = vec![
            Pause {
                id: 1,
                start: NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                end: Some(NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(10, 15, 0).unwrap())),
                duration: Some(Duration::minutes(15)),
            }
        ];
        let breaks = vec![];
        
        let productivity = productivity::calculate_productivity(&workday, &pauses, &breaks);
        // 8 hours work, 15 minutes pause = 7:45 / 8:00 = 96.875%
        assert!((productivity - 96.875).abs() < 0.001);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_calculation_with_breaks(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();
        let pauses = vec![];
        let breaks = vec![
            Break {
                id: Some(1),
                date: workday.date,
                start: NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(12, 0, 0).unwrap()),
                end: NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(13, 0, 0).unwrap()),
                duration: Duration::hours(1),
                reason: Some("Lunch break".to_string()),
                created_at: Some(NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(12, 0, 0).unwrap())),
            }
        ];
        
        let productivity = productivity::calculate_productivity(&workday, &pauses, &breaks);
        // 8 hours total, 1 hour break excluded = 7/7 = 100%
        assert_eq!(productivity, 100.0);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_calculation_with_pauses_and_breaks(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();
        let pauses = vec![
            Pause {
                id: 1,
                start: NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                end: Some(NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(10, 15, 0).unwrap())),
                duration: Some(Duration::minutes(15)),
            }
        ];
        let breaks = vec![
            Break {
                id: Some(1),
                date: workday.date,
                start: NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(12, 0, 0).unwrap()),
                end: NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(13, 0, 0).unwrap()),
                duration: Duration::hours(1),
                reason: Some("Lunch break".to_string()),
                created_at: Some(NaiveDateTime::new(workday.date, NaiveTime::from_hms_opt(12, 0, 0).unwrap())),
            }
        ];
        
        let productivity = productivity::calculate_productivity(&workday, &pauses, &breaks);
        // 8 hours total, 1 hour break excluded, 15 minutes pause
        // Net work: 7 hours - 15 minutes = 6:45
        // Available: 7 hours (excluding break)
        // Productivity: 6:45 / 7:00 = 96.42857%
        assert!((productivity - 96.42857).abs() < 0.001);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_for_intervals_no_pauses(_ctx: &mut ProductivityTestContext) {
        let work_time = Duration::hours(6);
        let pauses = vec![];
        let breaks = vec![];
        
        let productivity = productivity::calculate_productivity_for_intervals(&work_time, &pauses, &breaks);
        assert_eq!(productivity, 100.0);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_for_intervals_with_pauses(_ctx: &mut ProductivityTestContext) {
        let work_time = Duration::hours(6);
        let pauses = vec![
            Pause {
                id: 1,
                start: NaiveDate::from_ymd_opt(2022, 1, 15).unwrap().and_hms_opt(10, 0, 0).unwrap(),
                end: Some(NaiveDate::from_ymd_opt(2022, 1, 15).unwrap().and_hms_opt(10, 15, 0).unwrap()),
                duration: Some(Duration::minutes(15)),
            }
        ];
        let breaks = vec![];
        
        let productivity = productivity::calculate_productivity_for_intervals(&work_time, &pauses, &breaks);
        // 6 hours work, 15 minutes pause = 5:45 / 6:00 = 95.833%
        assert!((productivity - 95.833333).abs() < 0.001);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_edge_cases(_ctx: &mut ProductivityTestContext) {
        // Test zero work time
        let zero_work = Duration::zero();
        let productivity = productivity::calculate_productivity_for_intervals(&zero_work, &[], &[]);
        assert_eq!(productivity, 0.0);
        
        // Test productivity clamping (should not exceed 100%)
        let work_time = Duration::hours(1);
        let negative_pause = vec![
            Pause {
                id: 1,
                start: NaiveDate::from_ymd_opt(2022, 1, 15).unwrap().and_hms_opt(10, 0, 0).unwrap(),
                end: Some(NaiveDate::from_ymd_opt(2022, 1, 15).unwrap().and_hms_opt(9, 0, 0).unwrap()), // End before start (invalid)
                duration: Some(Duration::hours(-2)), // Negative duration
            }
        ];
        
        let productivity = productivity::calculate_productivity_for_intervals(&work_time, &negative_pause, &[]);
        assert!(productivity >= 0.0 && productivity <= 100.0);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_boundary_values(_ctx: &mut ProductivityTestContext) {
        let workday = create_test_workday();
        
        // Test with pauses equal to work time (should be 0% productivity)
        let massive_pause = vec![
            Pause {
                id: 1,
                start: workday.start,
                end: workday.end,
                duration: Some(Duration::hours(8)),
            }
        ];
        
        let productivity = productivity::calculate_productivity(&workday, &massive_pause, &[]);
        assert_eq!(productivity, 0.0);
        
        // Test with break equal to work time (should be 0% productivity due to no available time)
        let massive_break = vec![
            Break {
                id: Some(1),
                date: workday.date,
                start: workday.start,
                end: workday.end.unwrap(),
                duration: Duration::hours(8),
                reason: Some("All day break".to_string()),
                created_at: Some(workday.start),
            }
        ];
        
        let productivity = productivity::calculate_productivity(&workday, &[], &massive_break);
        assert_eq!(productivity, 0.0);
    }
}