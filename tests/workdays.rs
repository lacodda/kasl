#[cfg(test)]
mod tests {
    use chrono::{Local, NaiveDate};
    use kasl::db::workdays::Workdays;
    use serial_test::serial;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    /// Test context to ensure a clean database for each workday test.
    struct WorkdayTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for WorkdayTestContext {
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
            WorkdayTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(WorkdayTestContext)]
    #[serial]
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
    #[serial]
    #[test]
    fn test_fetch_nonexistent_workday(_ctx: &mut WorkdayTestContext) {
        let mut workdays = Workdays::new().unwrap();
        let date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let workday = workdays.fetch(date).unwrap();
        assert!(workday.is_none());
    }

    #[test_context(WorkdayTestContext)]
    #[serial]
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
    #[serial]
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

    #[test_context(WorkdayTestContext)]
    #[serial]
    #[test]
    fn ending_a_day_that_was_never_started_writes_nothing(_ctx: &mut WorkdayTestContext) {
        // Both halves matter. The storage side: ending a day nobody started
        // must not conjure a workday record. The answer side: it must come
        // back as "no day", because for a long time it came back as plain Ok
        // and `kasl end` printed "Workday ended for today" over an empty
        // database. The fix had to change the return value, not the data.
        let mut workdays = Workdays::new().unwrap();
        let date = Local::now().date_naive();

        assert!(!workdays.insert_end(date).unwrap(), "a day that was never started must report as missing");
        assert!(
            workdays.fetch(date).unwrap().is_none(),
            "ending an unstarted day must not conjure a workday record"
        );

        // And the other side of the same answer: a day that exists closes.
        workdays.insert_start(date).unwrap();
        assert!(workdays.insert_end(date).unwrap(), "an open day must report as closed");
    }

    /// Records a day on each of `dates`, in a scrambled order.
    ///
    /// Inserted out of order on purpose: `recorded_dates` promises oldest
    /// first, and a test that inserted in order would pass against a query
    /// that only ever returned insertion order.
    ///
    /// Worth knowing what this does *not* prove. `date` is `UNIQUE`, so
    /// SQLite answers the range from that index and hands back sorted rows
    /// whether or not the query says `ORDER BY` - removing the clause leaves
    /// these tests green. The clause stays because the ordering is a promise
    /// of the method rather than a property of today's schema: drop the
    /// uniqueness and the sorting goes with it, silently. What the scrambled
    /// insert does catch is the ordering being taken from insertion order,
    /// which is what a hand-rolled loop would have produced.
    fn record(workdays: &mut Workdays, dates: &[&str]) {
        for date in dates {
            workdays.insert_start(NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap()).unwrap();
        }
    }

    #[test_context(WorkdayTestContext)]
    #[serial]
    #[test]
    fn recorded_dates_answers_the_whole_history_oldest_first(_ctx: &mut WorkdayTestContext) {
        // What `kasl server backfill` without --from asks for: everything this
        // machine ever recorded, in the order it happened.
        let mut workdays = Workdays::new().unwrap();
        record(&mut workdays, &["2026-03-04", "2025-11-30", "2026-01-15"]);

        let dates = workdays.recorded_dates(None, None).unwrap();

        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2025, 11, 30).unwrap(),
                NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
                NaiveDate::from_ymd_opt(2026, 3, 4).unwrap(),
            ]
        );
    }

    #[test_context(WorkdayTestContext)]
    #[serial]
    #[test]
    fn recorded_dates_holds_both_bounds_inclusively(_ctx: &mut WorkdayTestContext) {
        // Off by one at either end sends a day that was not asked for, or
        // leaves one behind. Both ends are named by a day that sits exactly
        // on them.
        let mut workdays = Workdays::new().unwrap();
        record(&mut workdays, &["2026-01-31", "2026-02-01", "2026-02-15", "2026-02-28", "2026-03-01"]);

        let dates = workdays
            .recorded_dates(
                Some(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()),
                Some(NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()),
            )
            .unwrap();

        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 2, 15).unwrap(),
                NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(),
            ],
            "both bounds are inclusive, and nothing outside them comes along"
        );
    }

    #[test_context(WorkdayTestContext)]
    #[serial]
    #[test]
    fn recorded_dates_leaves_out_the_days_never_worked(_ctx: &mut WorkdayTestContext) {
        // The reason backfill reads the table rather than walking the
        // calendar: a range full of weekends must not queue days that were
        // never worked and can never be sent.
        let mut workdays = Workdays::new().unwrap();
        record(&mut workdays, &["2026-02-02", "2026-02-06"]);

        let dates = workdays
            .recorded_dates(
                Some(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()),
                Some(NaiveDate::from_ymd_opt(2026, 2, 8).unwrap()),
            )
            .unwrap();

        assert_eq!(dates.len(), 2, "only the two days with a workday row: {:?}", dates);
    }

    #[test_context(WorkdayTestContext)]
    #[serial]
    #[test]
    fn recorded_dates_on_an_empty_database_is_empty(_ctx: &mut WorkdayTestContext) {
        // The machine that has never recorded a day. It gets its own sentence
        // from backfill, so the empty answer has to be distinguishable rather
        // than an error.
        let mut workdays = Workdays::new().unwrap();

        assert!(workdays.recorded_dates(None, None).unwrap().is_empty());
    }

    #[test_context(WorkdayTestContext)]
    #[serial]
    #[test]
    fn recorded_dates_compares_dates_rather_than_their_spelling(_ctx: &mut WorkdayTestContext) {
        // The bounds go into SQL as text, so this is the check that the text
        // compares like a date. Single-digit months and days are where a
        // string comparison would part company with a calendar - "2026-9-01"
        // sorts after "2026-10-01" - and the storage format is what keeps
        // them agreeing.
        let mut workdays = Workdays::new().unwrap();
        record(&mut workdays, &["2026-09-30", "2026-10-01"]);

        let dates = workdays.recorded_dates(Some(NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()), None).unwrap();

        assert_eq!(
            dates,
            vec![NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),]
        );
    }
}
