#[cfg(test)]
mod tests {
    use kasl::db::templates::{TaskTemplate, Templates};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    struct TemplateTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for TemplateTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            TemplateTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(TemplateTestContext)]
    #[test]
    fn test_template_crud(_ctx: &mut TemplateTestContext) {
        let mut templates = Templates::new().unwrap();

        // Create template
        let template = TaskTemplate::new(
            "daily-standup".to_string(),
            "Daily standup meeting".to_string(),
            "Discuss progress and blockers".to_string(),
            100,
        );
        templates.create(&template).unwrap();

        // Read template
        let fetched = templates.get("daily-standup").unwrap().unwrap();
        assert_eq!(fetched.name, "daily-standup");
        assert_eq!(fetched.task_name, "Daily standup meeting");

        // Update template
        let mut updated = fetched;
        updated.task_name = "Updated standup".to_string();
        templates.update(&updated).unwrap();

        // Verify update
        let verified = templates.get("daily-standup").unwrap().unwrap();
        assert_eq!(verified.task_name, "Updated standup");

        // Delete template
        templates.delete("daily-standup").unwrap();
        assert!(templates.get("daily-standup").unwrap().is_none());
    }

    #[test_context(TemplateTestContext)]
    #[test]
    fn test_template_search(_ctx: &mut TemplateTestContext) {
        let mut templates = Templates::new().unwrap();

        // Create multiple templates
        let template1 = TaskTemplate::new("meeting-template".to_string(), "Team meeting".to_string(), "".to_string(), 100);
        let template2 = TaskTemplate::new("code-review".to_string(), "Code review session".to_string(), "".to_string(), 50);

        templates.create(&template1).unwrap();
        templates.create(&template2).unwrap();

        // Search by partial name
        let results = templates.search("meet").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "meeting-template");

        // Search by task name
        let results = templates.search("review").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "code-review");
    }
}
