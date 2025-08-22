#[cfg(test)]
mod tests {
    use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
    use kasl::libs::report::{filter_short_intervals, WorkInterval};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    struct ReportFunctionTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for ReportFunctionTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            ReportFunctionTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(ReportFunctionTestContext)]
    #[test]
    fn test_filter_short_intervals(_ctx: &mut ReportFunctionTestContext) {
        // Create test intervals - some short, some long
        let date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let intervals = vec![
            WorkInterval {
                start: NaiveDateTime::new(date, NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
                end: NaiveDateTime::new(date, NaiveTime::from_hms_opt(9, 10, 0).unwrap()),
                duration: Duration::minutes(10), // Short interval
                pause_after: Some(0),
            },
            WorkInterval {
                start: NaiveDateTime::new(date, NaiveTime::from_hms_opt(9, 15, 0).unwrap()),
                end: NaiveDateTime::new(date, NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                duration: Duration::minutes(45), // Long interval
                pause_after: Some(1),
            },
            WorkInterval {
                start: NaiveDateTime::new(date, NaiveTime::from_hms_opt(10, 5, 0).unwrap()),
                end: NaiveDateTime::new(date, NaiveTime::from_hms_opt(10, 10, 0).unwrap()),
                duration: Duration::minutes(5), // Very short interval
                pause_after: None,
            },
        ];

        // Filter with 30-minute minimum
        let (filtered, info) = filter_short_intervals(&intervals, 30);

        // Should keep only the 45-minute interval
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].duration, Duration::minutes(45));

        // Should have info about filtered intervals
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.count, 2); // 10-minute and 5-minute intervals filtered
        assert_eq!(info.total_duration, Duration::minutes(15)); // 10 + 5 minutes
    }

    #[test_context(ReportFunctionTestContext)]
    #[test]
    fn test_filter_short_intervals_none_filtered(_ctx: &mut ReportFunctionTestContext) {
        // Create test intervals - all long enough
        let date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let intervals = vec![
            WorkInterval {
                start: NaiveDateTime::new(date, NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
                end: NaiveDateTime::new(date, NaiveTime::from_hms_opt(9, 45, 0).unwrap()),
                duration: Duration::minutes(45),
                pause_after: Some(0),
            },
            WorkInterval {
                start: NaiveDateTime::new(date, NaiveTime::from_hms_opt(10, 0, 0).unwrap()),
                end: NaiveDateTime::new(date, NaiveTime::from_hms_opt(11, 0, 0).unwrap()),
                duration: Duration::minutes(60),
                pause_after: None,
            },
        ];

        // Filter with 30-minute minimum
        let (filtered, info) = filter_short_intervals(&intervals, 30);

        // Should keep all intervals
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].duration, Duration::minutes(45));
        assert_eq!(filtered[1].duration, Duration::minutes(60));

        // Should have no info about filtered intervals
        assert!(info.is_none());
    }
}