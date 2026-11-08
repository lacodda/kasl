#[cfg(test)]
mod tests {
    use chrono::Local;
    use kasl::db::tasks::Tasks;
    use kasl::db::workdays::Workdays;
    use kasl::libs::export::{ExportData, ExportFormat, Exporter};
    use kasl::libs::task::Task;
    use serial_test::serial;
    use tempfile::TempDir;
    use test_context::{AsyncTestContext, test_context};

    struct ExportTestContext {
        temp_dir: TempDir,
    }

    impl AsyncTestContext for ExportTestContext {
        async fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("HOME", temp_dir.path());
            }
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("LOCALAPPDATA", temp_dir.path());
            }
            ExportTestContext { temp_dir }
        }
    }

    #[test_context(ExportTestContext)]
    #[serial]
    #[tokio::test]
    async fn test_export_csv(ctx: &mut ExportTestContext) {
        // Setup test data
        let date = Local::now().date_naive();
        let mut workdays = Workdays::new().unwrap();
        workdays.insert_start(date).unwrap();

        let mut tasks = Tasks::new().unwrap();
        let task = Task::new("Test task", "Test comment", Some(75));
        tasks.insert(&task).unwrap();

        // Export to CSV
        let output_path = ctx.temp_dir.path().join("test_export.csv");
        let exporter = Exporter::new(ExportFormat::Csv, Some(output_path.clone()));
        exporter.export(ExportData::Tasks, date).await.unwrap();

        // Verify file exists
        assert!(output_path.exists());

        // Read and verify content
        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("Test task"));
        assert!(content.contains("75%"));
    }

    #[test_context(ExportTestContext)]
    #[serial]
    #[tokio::test]
    async fn test_export_json(ctx: &mut ExportTestContext) {
        let date = Local::now().date_naive();
        let mut workdays = Workdays::new().unwrap();
        workdays.insert_start(date).unwrap();

        // Export to JSON
        let output_path = ctx.temp_dir.path().join("test_export.json");
        let exporter = Exporter::new(ExportFormat::Json, Some(output_path.clone()));
        exporter.export(ExportData::Summary, date).await.unwrap();

        // Verify file exists and is valid JSON
        assert!(output_path.exists());
        let content = std::fs::read_to_string(&output_path).unwrap();
        let _: serde_json::Value = serde_json::from_str(&content).unwrap();
    }

    #[test_context(ExportTestContext)]
    #[serial]
    #[tokio::test]
    async fn the_exported_month_counts_hours_as_sum_does(ctx: &mut ExportTestContext) {
        // A past day of eight hours with an hour's lunch. The export used to
        // say 8:00 - the day's end minus its start - while `sum` said the
        // work, so the same day had two lengths.
        let date = chrono::NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let db = kasl::db::db::Db::new().unwrap();
        db.conn
            .execute(
                "INSERT INTO workdays (date, start, end) VALUES ('2026-03-10', '2026-03-10 09:00:00', '2026-03-10 17:00:00')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO pauses (start, end, duration) VALUES ('2026-03-10 12:00:00', '2026-03-10 13:00:00', 3600)",
                [],
            )
            .unwrap();

        let output_path = ctx.temp_dir.path().join("month.json");
        Exporter::new(ExportFormat::Json, Some(output_path.clone()))
            .export(ExportData::Summary, date)
            .await
            .unwrap();
        let exported: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&output_path).unwrap()).unwrap();

        let month = kasl::libs::summary::Summary::month(date).await.unwrap();
        let sum_says = kasl::libs::formatter::format_duration(&month.total);
        assert_eq!(exported["total_hours"], serde_json::json!(sum_says), "{exported:#}");
        assert_ne!(exported["total_hours"], serde_json::json!("08:00"), "the lunch is not work: {exported:#}");
        assert_eq!(exported["total_days"], serde_json::json!(1));
    }

    #[test_context(ExportTestContext)]
    #[serial]
    #[tokio::test]
    async fn test_export_excel(ctx: &mut ExportTestContext) {
        let date = Local::now().date_naive();
        let mut tasks = Tasks::new().unwrap();
        let task = Task::new("Excel test", "Comment", Some(100));
        tasks.insert(&task).unwrap();

        // Export to Excel
        let output_path = ctx.temp_dir.path().join("test_export.xlsx");
        let exporter = Exporter::new(ExportFormat::Excel, Some(output_path.clone()));
        exporter.export(ExportData::Tasks, date).await.unwrap();

        // Verify file exists and has content
        assert!(output_path.exists());
        let metadata = std::fs::metadata(&output_path).unwrap();
        assert!(metadata.len() > 0);
    }
}
