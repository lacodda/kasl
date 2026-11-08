#[cfg(test)]
mod tests {
    use kasl::db::tags::{Tag, Tags};
    use kasl::db::tasks::Tasks;
    use kasl::libs::task::Task;
    use serial_test::serial;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    struct TagTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for TagTestContext {
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
            TagTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(TagTestContext)]
    #[serial]
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
    #[serial]
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
    #[serial]
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

    #[test_context(TagTestContext)]
    #[serial]
    #[test]
    fn a_tag_stays_with_the_task_into_its_next_day(_ctx: &mut TagTestContext) {
        // The bug: a task continued into a new day got a fresh row with no
        // tags, so `task list --tag payments` lost it from its second day on.
        let mut tasks = Tasks::new().unwrap();
        let first = tasks.insert(&Task::new("Payments refund", "", Some(40))).unwrap().id.unwrap();
        let mut tags = Tags::new().unwrap();
        let payments = tags.get_or_create_tags(&["payments".to_string()]).unwrap()[0];
        tags.add_tag_to_task(first, payments).unwrap();

        let mut next_day = Task::new("Payments refund", "", Some(80));
        next_day.task_id = Some(first);
        let second = tasks.insert(&next_day).unwrap().id.unwrap();

        let names: Vec<String> = tags.get_tags_by_task(second).unwrap().into_iter().map(|tag| tag.name).collect();
        assert_eq!(names, ["payments"], "the second day's row carries the task's tag");
        let tagged: Vec<Option<i32>> = tasks
            .fetch(kasl::libs::task::TaskFilter::ByTag("payments".to_string()))
            .unwrap()
            .iter()
            .map(|task| task.id)
            .collect();
        assert_eq!(tagged, [Some(first), Some(second)]);
        assert_eq!(tags.count_tasks_with_tag(payments).unwrap(), 1, "one task, however many days");

        // A tag taken off any day's row comes off the task.
        tags.remove_tag_from_task(second, payments).unwrap();
        assert!(tags.get_tags_by_task(first).unwrap().is_empty());

        // And the task's tags go when its last row does.
        tags.add_tag_to_task(first, payments).unwrap();
        tasks.delete(first).unwrap();
        assert_eq!(tags.count_tasks_with_tag(payments).unwrap(), 1, "a row of the task is still there");
        tasks.delete(second).unwrap();
        assert_eq!(tags.count_tasks_with_tag(payments).unwrap(), 0);
    }
}
