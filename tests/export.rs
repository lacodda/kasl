#[cfg(test)]
mod tests {
    use chrono::Local;
    use kasl::db::tasks::Tasks;
    use kasl::db::workdays::Workdays;
    use kasl::libs::export::{ExportData, ExportFormat, Exporter};
    use kasl::libs::task::Task;
    use tempfile::TempDir;
    use test_context::{test_context, AsyncTestContext};

    struct ExportTestContext {
        temp_dir: TempDir,
    }

    impl AsyncTestContext for ExportTestContext {
        async fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            ExportTestContext { temp_dir }
        }
    }

    #[test_context(ExportTestContext)]
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
