#[cfg(test)]
mod tests {
    use chrono::{Duration, NaiveDate};
    use kasl::db::workdays::Workday;
    use kasl::libs::report::{analyze_short_intervals, calculate_work_intervals};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    struct IntervalTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for IntervalTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            IntervalTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(IntervalTestContext)]
    #[test]
    fn test_short_interval_detection(_ctx: &mut IntervalTestContext) {
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let start = date.and_hms_opt(9, 0, 0).unwrap();
        let end = date.and_hms_opt(17, 0, 0).unwrap();

        let workday = Workday {
            id: 1,
            date,
            start,
            end: Some(end),
        };

        // Create pauses that will result in a 5-minute interval
        let pauses = vec![
            kasl::libs::pause::Pause {
                id: 1,
                start: date.and_hms_opt(10, 0, 0).unwrap(),
                end: Some(date.and_hms_opt(10, 30, 0).unwrap()),
                duration: Some(Duration::minutes(30)),
            },
            kasl::libs::pause::Pause {
                id: 2,
                start: date.and_hms_opt(10, 35, 0).unwrap(),
                end: Some(date.and_hms_opt(12, 0, 0).unwrap()),
                duration: Some(Duration::minutes(85)),
            },
        ];

        let intervals = calculate_work_intervals(&workday, &pauses);
        let analysis = analyze_short_intervals(&intervals, 10);

        assert!(analysis.is_some());
        let analysis = analysis.unwrap();
        assert_eq!(analysis.intervals.len(), 1);
        assert_eq!(analysis.intervals[0].1.duration.num_minutes(), 5);
    }

    #[test_context(IntervalTestContext)]
    #[test]
    fn test_no_short_intervals(_ctx: &mut IntervalTestContext) {
        let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let workday = Workday {
            id: 1,
            date,
            start: date.and_hms_opt(9, 0, 0).unwrap(),
            end: Some(date.and_hms_opt(17, 0, 0).unwrap()),
        };

        // Create pauses that result in longer intervals
        let pauses = vec![kasl::libs::pause::Pause {
            id: 1,
            start: date.and_hms_opt(12, 0, 0).unwrap(),
            end: Some(date.and_hms_opt(13, 0, 0).unwrap()),
            duration: Some(Duration::hours(1)),
        }];

        let intervals = calculate_work_intervals(&workday, &pauses);
        let analysis = analyze_short_intervals(&intervals, 10);

        assert!(analysis.is_none());
    }
}
