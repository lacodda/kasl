#[cfg(test)]
mod tests {
    use kasl::db::tasks::Tasks;
    use kasl::libs::task::{Task, TaskFilter};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    struct TaskTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for TaskTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            TaskTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(TaskTestContext)]
    #[test]
    fn test_task_delete(_ctx: &mut TaskTestContext) {
        let mut tasks = Tasks::new().unwrap();

        // Create a task
        let task = Task::new("Test task", "Comment", Some(50));
        tasks.insert(&task).unwrap();
        let created_tasks = tasks.get().unwrap();
        assert_eq!(created_tasks.len(), 1);
        let task_id = created_tasks[0].id.unwrap();

        // Delete the task
        let deleted = tasks.delete(task_id).unwrap();
        assert_eq!(deleted, 1);

        // Verify it's deleted
        let remaining = tasks.fetch(TaskFilter::All).unwrap();
        assert_eq!(remaining.len(), 0);
    }

    #[test_context(TaskTestContext)]
    #[test]
    fn test_task_update(_ctx: &mut TaskTestContext) {
        let mut tasks = Tasks::new().unwrap();

        // Create a task
        let mut task = Task::new("Original name", "Original comment", Some(0));
        tasks.insert(&task).unwrap();
        let created_tasks = tasks.get().unwrap();
        task = created_tasks[0].clone();

        // Update the task
        task.name = "Updated name".to_string();
        task.comment = "Updated comment".to_string();
        task.completeness = Some(100);
        tasks.update(&task).unwrap();

        // Verify the update
        let updated = tasks.get_by_id(task.id.unwrap()).unwrap().unwrap();
        assert_eq!(updated.name, "Updated name");
        assert_eq!(updated.comment, "Updated comment");
        assert_eq!(updated.completeness, Some(100));
    }

    #[test_context(TaskTestContext)]
    #[test]
    fn test_task_delete_many(_ctx: &mut TaskTestContext) {
        let mut tasks = Tasks::new().unwrap();

        // Create multiple tasks
        for i in 1..=5 {
            let task = Task::new(&format!("Task {}", i), "", Some(100));
            tasks.insert(&task).unwrap();
        }

        // Get all task IDs
        let all_tasks = tasks.fetch(TaskFilter::All).unwrap();
        let ids: Vec<i32> = all_tasks.iter().filter_map(|t| t.id).collect();
        assert_eq!(ids.len(), 5);

        // Delete first 3 tasks
        let deleted = tasks.delete_many(&ids[..3]).unwrap();
        assert_eq!(deleted, 3);

        // Verify remaining tasks
        let remaining = tasks.fetch(TaskFilter::All).unwrap();
        assert_eq!(remaining.len(), 2);
    }
}
