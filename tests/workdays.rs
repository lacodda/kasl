#[cfg(test)]
mod tests {
    use chrono::{Local, NaiveDate};
    use kasl::db::workdays::Workdays;
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    /// Test context to ensure a clean database for each workday test.
    struct WorkdayTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for WorkdayTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            WorkdayTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(WorkdayTestContext)]
    #[test]
    fn test_insert_and_fetch_workday(_ctx: &mut WorkdayTestContext) {
        let mut workdays = Workdays::new().unwrap();
        let date = Local::now().date_naive();

        // Insert start
        workdays.insert_start(date).unwrap();
        let workday = workdays.fetch(date).unwrap().unwrap();
        assert_eq!(workday.date, date);
        assert!(workday.start <= Local::now().naive_local());
        assert!(workday.end.is_none());

        // Insert end
        workdays.insert_end(date).unwrap();
        let workday = workdays.fetch(date).unwrap().unwrap();
        assert!(workday.end.is_some());
    }

    #[test_context(WorkdayTestContext)]
    #[test]
    fn test_fetch_nonexistent_workday(_ctx: &mut WorkdayTestContext) {
        let mut workdays = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let workday = workdays.fetch(date).unwrap();
        assert!(workday.is_none());
    }

    #[test_context(WorkdayTestContext)]
    #[test]
    fn test_fetch_month(_ctx: &mut WorkdayTestContext) {
        let mut workdays = Workdays::new().unwrap();
        let date1 = NaiveDate::from_ymd_opt(2025, 6, 1).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2025, 6, 2).unwrap();
        let date_other_month = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

        workdays.insert_start(date1).unwrap();
        workdays.insert_start(date2).unwrap();
        workdays.insert_start(date_other_month).unwrap();

        let workdays_list = workdays.fetch_month(date1).unwrap();
        assert_eq!(workdays_list.len(), 2);
        assert_eq!(workdays_list[0].date, date1);
        assert_eq!(workdays_list[1].date, date2);
        assert!(!workdays_list.iter().any(|wd| wd.date == date_other_month));
    }

    #[test_context(WorkdayTestContext)]
    #[test]
    fn test_insert_start_is_idempotent(_ctx: &mut WorkdayTestContext) {
        let mut workdays = Workdays::new().unwrap();
        let date = Local::now().date_naive();

        // First insert
        workdays.insert_start(date).unwrap();
        let workday1 = workdays.fetch(date).unwrap().unwrap();

        // Second insert should do nothing
        workdays.insert_start(date).unwrap();
        let workday2 = workdays.fetch(date).unwrap().unwrap();

        // The start time should not have changed
        assert_eq!(workday1.start, workday2.start);
    }
}
