#[cfg(test)]
mod tests {
    use kasl::db::db::Db;
    use kasl::db::tasks::Tasks;
    use kasl::db::workdays::Workdays;
    use kasl::libs::task::Task;
    use kasl::libs::config::Config;
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};
    use chrono::Utc;

    struct CommandTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for CommandTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            
            CommandTestContext {
                _temp_dir: temp_dir,
            }
        }
    }

    #[test_context(CommandTestContext)]
    #[test]
    fn test_init_command_creates_config(_ctx: &mut CommandTestContext) {
        // Test that we can create and save a configuration
        let config = Config::default();
        let result = config.save();
        assert!(result.is_ok());
    }

    #[test_context(CommandTestContext)]
    #[test]
    fn test_task_basic_operations(_ctx: &mut CommandTestContext) {
        // Initialize database
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Test creating a task
        let task = Task::new("Test Task", "Test task description", Some(50));
        let result = tasks.insert(&task);
        assert!(result.is_ok());
        
        // Verify task was created
        let fetched_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        assert!(!fetched_tasks.is_empty());
        assert_eq!(fetched_tasks[0].name, "Test Task");
    }

    #[test_context(CommandTestContext)]
    #[test]
    fn test_workday_basic_operations(_ctx: &mut CommandTestContext) {
        // Initialize database
        let _db = Db::new().unwrap();
        let mut workdays = Workdays::new().unwrap();
        
        // Test creating a workday
        let today = Utc::now().date_naive();
        let result = workdays.insert_start(today);
        assert!(result.is_ok());
        
        // Test ending a workday
        let end_result = workdays.insert_end(today);
        assert!(end_result.is_ok());
        
        // Verify workday was created
        let workday = workdays.fetch(today).unwrap();
        assert!(workday.is_some());
        assert!(workday.unwrap().end.is_some());
    }

    #[test_context(CommandTestContext)]
    #[test]
    fn test_database_initialization(_ctx: &mut CommandTestContext) {
        // Test that database can be initialized without errors
        let db_result = Db::new();
        assert!(db_result.is_ok());
    }

    #[test_context(CommandTestContext)]
    #[test]
    fn test_config_operations(_ctx: &mut CommandTestContext) {
        // Test config creation and save/load
        let config = Config::default();
        let save_result = config.save();
        assert!(save_result.is_ok());
        
        let loaded_config = Config::read();
        assert!(loaded_config.is_ok());
    }

    #[test_context(CommandTestContext)]
    #[test]
    fn test_task_filtering(_ctx: &mut CommandTestContext) {
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Create tasks with different completion levels
        let task1 = Task::new("Complete Task", "Fully done", Some(100));
        let task2 = Task::new("Incomplete Task", "Work in progress", Some(50));
        
        let _insert1 = tasks.insert(&task1);
        let _insert2 = tasks.insert(&task2);
        
        // Test fetching all tasks
        let all_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        assert_eq!(all_tasks.len(), 2);
        
        // Test fetching today's tasks
        let today = Utc::now().date_naive();
        let today_tasks = tasks.fetch(kasl::libs::task::TaskFilter::Date(today)).unwrap();
        assert_eq!(today_tasks.len(), 2);
    }

    #[test_context(CommandTestContext)]
    #[test]
    fn test_error_handling(_ctx: &mut CommandTestContext) {
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Test that operations work even with edge cases
        let empty_name_task = Task::new("", "Empty name task", Some(50));
        let result = tasks.insert(&empty_name_task);
        // This should work - empty names might be allowed
        assert!(result.is_ok());
        
        // Test fetching non-existent task by ID
        let non_existent = tasks.get_by_id(99999).unwrap();
        assert!(non_existent.is_none());
    }
}