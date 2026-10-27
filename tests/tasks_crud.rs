#[cfg(test)]
mod tests {
    use kasl::db::tasks::Tasks;
    use kasl::libs::task::{Task, TaskFilter};
    use serial_test::serial;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    struct TaskTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for TaskTestContext {
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
            TaskTestContext { _temp_dir: temp_dir }
        }
    }

    /// Tasks made without a chain - taken from the inbox, imported by `task
    /// find` - used to share chain 0, so the incomplete query saw them as one
    /// task, and one finished import hid every unfinished taken issue.
    #[test_context(TaskTestContext)]
    #[serial]
    #[test]
    fn tasks_without_a_chain_each_start_their_own(_ctx: &mut TaskTestContext) {
        let mut tasks = Tasks::new().unwrap();
        tasks.insert(&Task::new("KA-1 First issue", "", Some(0)).from_issue("KA-1")).unwrap();
        tasks.insert(&Task::new("KA-2 Second issue", "", Some(30)).from_issue("KA-2")).unwrap();
        tasks.insert(&Task::new("Imported commit", "", Some(100))).unwrap();

        for task in tasks.fetch(TaskFilter::All).unwrap() {
            assert_eq!(task.task_id, task.id, "{} must be its own chain", task.name);
        }

        // Yesterday's work is what `task find` offers today.
        tasks
            .conn
            .execute("UPDATE tasks SET timestamp = datetime('now', 'localtime', '-1 day')", [])
            .unwrap();
        let mut open: Vec<String> = tasks.fetch(TaskFilter::Incomplete).unwrap().into_iter().map(|t| t.name).collect();
        open.sort();
        assert_eq!(open, ["KA-1 First issue", "KA-2 Second issue"], "each unfinished task is offered on its own");
    }

    #[test_context(TaskTestContext)]
    #[serial]
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
    #[serial]
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
    #[serial]
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
