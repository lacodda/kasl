#[cfg(test)]
mod tests {
    use kasl::db::tags::{Tag, Tags};
    use kasl::db::tasks::Tasks;
    use kasl::libs::task::Task;
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    struct TagTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for TagTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            TagTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(TagTestContext)]
    #[test]
    fn test_tag_crud(_ctx: &mut TagTestContext) {
        let mut tags = Tags::new().unwrap();

        // Create tag
        let tag = Tag::new("urgent".to_string(), Some("red".to_string()));
        let tag_id = tags.create(&tag).unwrap();
        assert!(tag_id > 0);

        // Read tag
        let fetched = tags.get_by_name("urgent").unwrap().unwrap();
        assert_eq!(fetched.name, "urgent");
        assert_eq!(fetched.color, Some("red".to_string()));

        // Update tag
        let mut tag = tags.get_by_id(tag_id).unwrap().unwrap();
        tag.name = "critical".to_string();
        tag.color = Some("orange".to_string());
        tags.update(&tag).unwrap();
        let updated = tags.get_by_id(tag_id).unwrap().unwrap();
        assert_eq!(updated.name, "critical");
        assert_eq!(updated.color, Some("orange".to_string()));

        // Delete tag
        tags.delete(tag_id).unwrap();
        assert!(tags.get_by_id(tag_id).unwrap().is_none());
    }

    #[test_context(TagTestContext)]
    #[test]
    fn test_task_tags(_ctx: &mut TagTestContext) {
        let mut tags = Tags::new().unwrap();
        let mut tasks = Tasks::new().unwrap();

        // Create tags
        let tag1 = Tag::new("backend".to_string(), None);
        let tag1_id = tags.create(&tag1).unwrap();

        let tag2 = Tag::new("bugfix".to_string(), None);
        let tag2_id = tags.create(&tag2).unwrap();

        // Create task
        let task = Task::new("Fix API bug", "", Some(0));
        tasks.insert(&task).unwrap();
        let task_id = tasks.get().unwrap()[0].id.unwrap();

        // Add tags to task
        tags.set_task_tags(task_id, &[tag1_id, tag2_id]).unwrap();

        // Verify tags
        let task_tags = tags.get_tags_by_task(task_id).unwrap();
        assert_eq!(task_tags.len(), 2);

        // Get tasks by tag
        let backend_tasks = tags.get_tasks_by_tag(tag1_id).unwrap();
        assert_eq!(backend_tasks.len(), 1);
        assert_eq!(backend_tasks[0], task_id);
    }

    #[test_context(TagTestContext)]
    #[test]
    fn test_get_or_create_tags(_ctx: &mut TagTestContext) {
        let mut tags = Tags::new().unwrap();

        // First call creates tags
        let tag_names = vec!["feature".to_string(), "ui".to_string()];
        let tag_ids1 = tags.get_or_create_tags(&tag_names).unwrap();
        assert_eq!(tag_ids1.len(), 2);

        // Second call returns existing tags
        let tag_ids2 = tags.get_or_create_tags(&tag_names).unwrap();
        assert_eq!(tag_ids1, tag_ids2);

        // Mixed: one existing, one new
        let mixed_names = vec!["ui".to_string(), "backend".to_string()];
        let mixed_ids = tags.get_or_create_tags(&mixed_names).unwrap();
        assert_eq!(mixed_ids.len(), 2);
        assert_eq!(mixed_ids[0], tag_ids1[1]); // "ui" already existed
    }
}
