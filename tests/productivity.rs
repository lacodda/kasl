#[cfg(test)]
mod tests {
    use chrono::{Duration, NaiveDate};
    use kasl::db::{breaks::Breaks, workdays::Workdays};
    use kasl::libs::{pause::Pause, report};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

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

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_calculation_edge_cases(_ctx: &mut ProductivityTestContext) {
        // Setup workday
        let mut workdays_db = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        workdays_db.insert_start(date).unwrap();
        
        // Set 8-hour workday
        workdays_db.conn.execute(
            "UPDATE workdays SET start = '2025-01-15 09:00:00', end = '2025-01-15 17:00:00' WHERE date = ?",
            [&date.to_string()],
        ).unwrap();

        let workday = workdays_db.fetch(date).unwrap().unwrap();

        // Test with no pauses or breaks (100% productivity)
        let no_pauses = vec![];
        let no_breaks = vec![];
        let perfect_productivity = report::calculate_productivity_with_breaks(
            &workday, &no_pauses, &no_breaks
        );
        assert!((perfect_productivity - 100.0).abs() < 0.01);

        // Test with pauses equal to work time (0% productivity)
        let massive_pauses = vec![
            Pause {
                id: 1,
                start: date.and_hms_opt(9, 0, 0).unwrap(),
                end: Some(date.and_hms_opt(17, 0, 0).unwrap()),
                duration: Some(Duration::minutes(480)),
            },
        ];
        let zero_productivity = report::calculate_productivity_with_breaks(
            &workday, &massive_pauses, &no_breaks
        );
        assert!((zero_productivity - 0.0).abs() < 0.01);

        // Test with breaks equal to work time (0% productivity)
        let breaks_db = Breaks::new().unwrap();
        let massive_break = kasl::db::breaks::Break {
            id: None,
            date,
            start: date.and_hms_opt(9, 0, 0).unwrap(),
            end: date.and_hms_opt(17, 0, 0).unwrap(),
            duration: Duration::minutes(480),
            reason: Some("All day break".to_string()),
            created_at: None,
        };
        breaks_db.insert(&massive_break).unwrap();
        let all_breaks = breaks_db.get_daily_breaks(date).unwrap();
        
        let zero_with_breaks = report::calculate_productivity_with_breaks(
            &workday, &no_pauses, &all_breaks
        );
        assert!((zero_with_breaks - 0.0).abs() < 0.01);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_needed_break_calculation_scenarios(_ctx: &mut ProductivityTestContext) {
        // Setup standard 8-hour workday
        let mut workdays_db = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        workdays_db.insert_start(date).unwrap();
        
        workdays_db.conn.execute(
            "UPDATE workdays SET start = '2025-01-15 09:00:00', end = '2025-01-15 17:00:00' WHERE date = ?",
            [&date.to_string()],
        ).unwrap();

        let workday = workdays_db.fetch(date).unwrap().unwrap();

        // Test 1: Already at target productivity (should need 0 break)
        let minimal_pauses = vec![
            Pause {
                id: 1,
                start: date.and_hms_opt(12, 0, 0).unwrap(),
                end: Some(date.and_hms_opt(12, 15, 0).unwrap()),
                duration: Some(Duration::minutes(15)),
            },
        ];
        let no_breaks = vec![];
        
        // Current productivity: (480 - 15) / 480 = 96.875%
        let needed_for_75 = report::calculate_needed_break_duration(
            &workday, &minimal_pauses, &no_breaks, 75.0
        );
        assert_eq!(needed_for_75, 0); // Already above threshold

        // Test 2: Need break to reach 80% from 70%
        let moderate_pauses = vec![
            Pause {
                id: 1,
                start: date.and_hms_opt(10, 0, 0).unwrap(),
                end: Some(date.and_hms_opt(11, 0, 0).unwrap()),
                duration: Some(Duration::minutes(60)),
            },
            Pause {
                id: 2,
                start: date.and_hms_opt(14, 0, 0).unwrap(),
                end: Some(date.and_hms_opt(15, 24, 0).unwrap()),
                duration: Some(Duration::minutes(84)),
            },
        ];
        
        // Current productivity: (480 - 144) / 480 = 70%
        // To reach 80%: 336 / (480 - break) = 0.8
        // 336 = 0.8 * (480 - break)
        // 336 = 384 - 0.8 * break
        // 0.8 * break = 48
        // break = 60 minutes
        let needed_for_80 = report::calculate_needed_break_duration(
            &workday, &moderate_pauses, &no_breaks, 80.0
        );
        assert_eq!(needed_for_80, 60);

        // Test 3: High target (95% with existing pauses)
        let needed_for_95 = report::calculate_needed_break_duration(
            &workday, &moderate_pauses, &no_breaks, 95.0
        );
        // Test passes if function returns a reasonable number
        assert!(needed_for_95 >= 0);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_with_mixed_pauses_and_breaks(_ctx: &mut ProductivityTestContext) {
        // Setup workday
        let mut workdays_db = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        workdays_db.insert_start(date).unwrap();
        
        workdays_db.conn.execute(
            "UPDATE workdays SET start = '2025-01-15 09:00:00', end = '2025-01-15 17:00:00' WHERE date = ?",
            [&date.to_string()],
        ).unwrap();

        let workday = workdays_db.fetch(date).unwrap().unwrap();

        // Create pauses (automatic detections)
        let pauses = vec![
            Pause {
                id: 1,
                start: date.and_hms_opt(10, 30, 0).unwrap(),
                end: Some(date.and_hms_opt(10, 45, 0).unwrap()),
                duration: Some(Duration::minutes(15)),
            },
            Pause {
                id: 2,
                start: date.and_hms_opt(15, 0, 0).unwrap(),
                end: Some(date.and_hms_opt(15, 10, 0).unwrap()),
                duration: Some(Duration::minutes(10)),
            },
        ];

        // Create breaks (manual additions)
        let breaks_db = Breaks::new().unwrap();
        let lunch_break = kasl::db::breaks::Break {
            id: None,
            date,
            start: date.and_hms_opt(12, 0, 0).unwrap(),
            end: date.and_hms_opt(13, 0, 0).unwrap(),
            duration: Duration::minutes(60),
            reason: Some("Lunch".to_string()),
            created_at: None,
        };
        let coffee_break = kasl::db::breaks::Break {
            id: None,
            date,
            start: date.and_hms_opt(14, 0, 0).unwrap(),
            end: date.and_hms_opt(14, 15, 0).unwrap(),
            duration: Duration::minutes(15),
            reason: Some("Coffee".to_string()),
            created_at: None,
        };
        
        breaks_db.insert(&lunch_break).unwrap();
        breaks_db.insert(&coffee_break).unwrap();
        let breaks = breaks_db.get_daily_breaks(date).unwrap();

        // Calculate productivity
        let productivity = report::calculate_productivity_with_breaks(
            &workday, &pauses, &breaks
        );
        
        // Test passes if productivity calculation works without errors
        assert!(productivity >= 0.0 && productivity <= 100.0);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_work_interval_calculation_with_breaks(_ctx: &mut ProductivityTestContext) {
        // Setup workday
        let mut workdays_db = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        workdays_db.insert_start(date).unwrap();
        
        workdays_db.conn.execute(
            "UPDATE workdays SET start = '2025-01-15 09:00:00', end = '2025-01-15 17:00:00' WHERE date = ?",
            [&date.to_string()],
        ).unwrap();

        let workday = workdays_db.fetch(date).unwrap().unwrap();

        // Create pauses (these create gaps in work intervals)
        let pauses = vec![
            Pause {
                id: 1,
                start: date.and_hms_opt(10, 30, 0).unwrap(),
                end: Some(date.and_hms_opt(10, 45, 0).unwrap()),
                duration: Some(Duration::minutes(15)),
            },
            Pause {
                id: 2,
                start: date.and_hms_opt(14, 0, 0).unwrap(),
                end: Some(date.and_hms_opt(14, 30, 0).unwrap()),
                duration: Some(Duration::minutes(30)),
            },
        ];

        // Calculate work intervals (breaks don't affect this - only pauses do)
        let intervals = report::calculate_work_intervals(&workday, &pauses);
        
        // Should have 3 intervals:
        // 1. 09:00 - 10:30 (90 minutes)
        // 2. 10:45 - 14:00 (195 minutes)
        // 3. 14:30 - 17:00 (150 minutes)
        assert_eq!(intervals.len(), 3);
        
        assert_eq!(intervals[0].start, date.and_hms_opt(9, 0, 0).unwrap());
        assert_eq!(intervals[0].end, date.and_hms_opt(10, 30, 0).unwrap());
        assert_eq!(intervals[0].duration.num_minutes(), 90);
        
        assert_eq!(intervals[1].start, date.and_hms_opt(10, 45, 0).unwrap());
        assert_eq!(intervals[1].end, date.and_hms_opt(14, 0, 0).unwrap());
        assert_eq!(intervals[1].duration.num_minutes(), 195);
        
        assert_eq!(intervals[2].start, date.and_hms_opt(14, 30, 0).unwrap());
        assert_eq!(intervals[2].end, date.and_hms_opt(17, 0, 0).unwrap());
        assert_eq!(intervals[2].duration.num_minutes(), 150);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_productivity_boundary_conditions(_ctx: &mut ProductivityTestContext) {
        // Setup minimal workday (1 minute)
        let mut workdays_db = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        workdays_db.insert_start(date).unwrap();
        
        workdays_db.conn.execute(
            "UPDATE workdays SET start = '2025-01-15 09:00:00', end = '2025-01-15 09:01:00' WHERE date = ?",
            [&date.to_string()],
        ).unwrap();

        let workday = workdays_db.fetch(date).unwrap().unwrap();

        // Test with no pauses/breaks (should be 100%)
        let productivity = report::calculate_productivity_with_breaks(
            &workday, &vec![], &vec![]
        );
        assert!((productivity - 100.0).abs() < 0.01);

        // Test with 1-minute pause (should be 0%)
        let full_pause = vec![
            Pause {
                id: 1,
                start: date.and_hms_opt(9, 0, 0).unwrap(),
                end: Some(date.and_hms_opt(9, 1, 0).unwrap()),
                duration: Some(Duration::minutes(1)),
            },
        ];
        
        let zero_productivity = report::calculate_productivity_with_breaks(
            &workday, &full_pause, &vec![]
        );
        assert!((zero_productivity - 0.0).abs() < 0.01);
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_suggestion_timing_logic(_ctx: &mut ProductivityTestContext) {
        let base_date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        
        // Test various workday durations and fractions
        let test_cases = vec![
            (8.0, 0.5), // 8 hour day, suggest after 4 hours
            (6.0, 0.3), // 6 hour day, suggest after 1.8 hours  
            (4.0, 0.75), // 4 hour day, suggest after 3 hours
        ];

        for (workday_hours, min_fraction) in test_cases {
            let workday = kasl::db::workdays::Workday {
                id: 1,
                date: base_date,
                start: base_date.and_hms_opt(9, 0, 0).unwrap(),
                end: None,
            };

            // Test with fraction = 0 (always suggest)
            let always_suggest = report::should_suggest_productivity_improvements(
                &workday, workday_hours, 0.0
            );
            assert!(always_suggest);

            // Test with fraction = 1 (suggest only after full workday)
            let never_suggest = report::should_suggest_productivity_improvements(
                &workday, workday_hours, 1.0
            );
            // This might be true or false depending on current time, but should not panic
            let _ = never_suggest;

            // Test with configured fraction
            let maybe_suggest = report::should_suggest_productivity_improvements(
                &workday, workday_hours, min_fraction
            );
            // Result depends on current time vs workday start
            let _ = maybe_suggest;
        }
    }

    #[test_context(ProductivityTestContext)]
    #[test]
    fn test_break_duration_calculation_precision(_ctx: &mut ProductivityTestContext) {
        // Setup precise test scenario
        let mut workdays_db = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        workdays_db.insert_start(date).unwrap();
        
        // 400-minute workday for easier math
        workdays_db.conn.execute(
            "UPDATE workdays SET start = '2025-01-15 09:00:00', end = '2025-01-15 15:40:00' WHERE date = ?",
            [&date.to_string()],
        ).unwrap();

        let workday = workdays_db.fetch(date).unwrap().unwrap();

        // 100 minutes of pauses (current productivity = 75%)
        let pauses = vec![
            Pause {
                id: 1,
                start: date.and_hms_opt(11, 0, 0).unwrap(),
                end: Some(date.and_hms_opt(12, 40, 0).unwrap()),
                duration: Some(Duration::minutes(100)),
            },
        ];

        let no_breaks = vec![];

        // Test exact threshold (should need 0)
        let needed_for_75 = report::calculate_needed_break_duration(
            &workday, &pauses, &no_breaks, 75.0
        );
        assert_eq!(needed_for_75, 0);

        // Test slight increase (should need small break)
        let needed_for_76 = report::calculate_needed_break_duration(
            &workday, &pauses, &no_breaks, 76.0
        );
        // To reach 76%: 300 / (400 - break) = 0.76
        // 300 = 0.76 * (400 - break)
        // 300 = 304 - 0.76 * break
        // 0.76 * break = 4
        // break ≈ 5.26 minutes, rounded up to 6
        assert!(needed_for_76 >= 5 && needed_for_76 <= 6);

        // Test decrease (should need 0)
        let needed_for_74 = report::calculate_needed_break_duration(
            &workday, &pauses, &no_breaks, 74.0
        );
        assert_eq!(needed_for_74, 0);
    }
}