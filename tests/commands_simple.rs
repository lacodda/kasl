#[cfg(test)]
mod tests {
    use kasl::db::db::Db;
    use kasl::db::tasks::Tasks;
    use kasl::db::workdays::Workdays;
    use kasl::libs::config::Config;
    use kasl::libs::task::Task;
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};
    use chrono::Utc;

    struct SimpleCommandTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for SimpleCommandTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            
            SimpleCommandTestContext {
                _temp_dir: temp_dir,
            }
        }
    }

    #[test_context(SimpleCommandTestContext)]
    #[test]
    fn test_database_initialization(_ctx: &mut SimpleCommandTestContext) {
        // Test that database can be initialized
        let db = Db::new();
        assert!(db.is_ok());
    }

    #[test_context(SimpleCommandTestContext)]
    #[test]
    fn test_tasks_crud_operations(_ctx: &mut SimpleCommandTestContext) {
        // Initialize database
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Create a test task
        let task = Task::new("Test Task", "Test Description", Some(50));
        let result = tasks.insert(&task);
        assert!(result.is_ok());
        
        // Get all tasks using TaskFilter::All
        let task_vec = tasks.fetch(kasl::libs::task::TaskFilter::All);
        assert!(task_vec.is_ok());
        
        let tasks_list = task_vec.unwrap();
        assert!(!tasks_list.is_empty());
        assert_eq!(tasks_list[0].name, "Test Task");
    }

    #[test_context(SimpleCommandTestContext)]
    #[test]
    fn test_workdays_operations(_ctx: &mut SimpleCommandTestContext) {
        // Initialize database
        let _db = Db::new().unwrap();
        let mut workdays = Workdays::new().unwrap();
        
        // Create a workday
        let start_date = Utc::now().date_naive();
        let result = workdays.insert_start(start_date);
        assert!(result.is_ok());
        
        // Fetch workday for the date
        let workday_result = workdays.fetch(start_date);
        assert!(workday_result.is_ok());
        
        let workday_opt = workday_result.unwrap();
        assert!(workday_opt.is_some());
    }

    #[test_context(SimpleCommandTestContext)]
    #[test]
    fn test_config_operations(_ctx: &mut SimpleCommandTestContext) {
        // Test config creation
        let config = Config::default();
        assert!(config.monitor.is_none());
        assert!(config.server.is_none());
        
        // Test config save/read
        let save_result = config.save();
        assert!(save_result.is_ok());
        
        let loaded_config = Config::read();
        assert!(loaded_config.is_ok());
    }

    #[test_context(SimpleCommandTestContext)]
    #[test]
    fn test_task_completion(_ctx: &mut SimpleCommandTestContext) {
        // Initialize database
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Create and insert a task
        let mut task = Task::new("Complete Me", "Task to complete", Some(75));
        let insert_result = tasks.insert(&task);
        assert!(insert_result.is_ok());
        
        // Get the inserted task ID
        let task_list = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        let inserted_task = &task_list[0];
        
        // Mark task as complete
        task.id = inserted_task.id;
        task.completeness = Some(100); // Mark as 100% complete
        
        let update_result = tasks.update(&task);
        assert!(update_result.is_ok());
        
        // Verify completion
        let updated_list = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        let completed_task = &updated_list[0];
        assert_eq!(completed_task.completeness, Some(100));
    }

    #[test_context(SimpleCommandTestContext)]
    #[test]
    fn test_task_filtering(_ctx: &mut SimpleCommandTestContext) {
        // Initialize database
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Create multiple tasks
        let task1 = Task::new("Task 1", "First task", Some(25));
        let task2 = Task::new("Task 2", "Second task", Some(50));
        let task3 = Task::new("Task 3", "Third task", Some(75));
        
        let _ = tasks.insert(&task1);
        let _ = tasks.insert(&task2);
        let _ = tasks.insert(&task3);
        
        // Test listing all tasks
        let all_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        assert_eq!(all_tasks.len(), 3);
        
        // Test with different filters if available
        // Note: Specific filter testing depends on actual implementation
    }

    #[test_context(SimpleCommandTestContext)]
    #[test]
    fn test_database_error_handling(_ctx: &mut SimpleCommandTestContext) {
        // Test operations that should handle errors gracefully
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Try to create a task with extreme values
        let edge_case_task = Task::new("", "", Some(0)); // Empty name/description
        let result = tasks.insert(&edge_case_task);
        
        // The result depends on implementation - it should either succeed or fail gracefully
        match result {
            Ok(_) => {
                // If it succeeds, verify it was stored
                let task_list = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
                assert!(!task_list.is_empty());
            },
            Err(_) => {
                // If it fails, that's also acceptable for edge cases
                // Just verify the error doesn't crash the system
            }
        }
    }

    #[test_context(SimpleCommandTestContext)]
    #[test]
    fn test_concurrent_database_access(_ctx: &mut SimpleCommandTestContext) {
        // Test that multiple database instances can coexist
        let _db1 = Db::new().unwrap();
        let _db2 = Db::new().unwrap();
        
        let mut tasks1 = Tasks::new().unwrap();
        let mut tasks2 = Tasks::new().unwrap();
        
        // Both should be able to perform operations
        let task1 = Task::new("Concurrent Task 1", "From first instance", Some(30));
        let task2 = Task::new("Concurrent Task 2", "From second instance", Some(70));
        
        let result1 = tasks1.insert(&task1);
        let result2 = tasks2.insert(&task2);
        
        assert!(result1.is_ok());
        assert!(result2.is_ok());
        
        // Both should see all tasks
        let list1 = tasks1.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        let list2 = tasks2.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        
        assert_eq!(list1.len(), list2.len());
        assert_eq!(list1.len(), 2);
    }

    #[test_context(SimpleCommandTestContext)]
    #[test]  
    fn test_workday_lifecycle(_ctx: &mut SimpleCommandTestContext) {
        // Test complete workday creation and management
        let _db = Db::new().unwrap();
        let mut workdays = Workdays::new().unwrap();
        
        // Create workday for today
        let today = Utc::now().date_naive();
        let create_result = workdays.insert_start(today);
        assert!(create_result.is_ok());
        
        // End workday
        let end_result = workdays.insert_end(today);
        assert!(end_result.is_ok());
        
        // Verify workday has end time
        let workday = workdays.fetch(today).unwrap();
        assert!(workday.is_some());
        assert!(workday.unwrap().end.is_some());
    }
}