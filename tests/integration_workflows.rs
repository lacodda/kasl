#[cfg(test)]
mod tests {
    use kasl::db::db::Db;
    use kasl::db::tasks::Tasks;
    use kasl::db::workdays::Workdays;
    use kasl::libs::task::Task;
    use kasl::libs::config::Config;
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};
    use chrono::{Duration, Utc};

    struct IntegrationTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for IntegrationTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            
            IntegrationTestContext {
                _temp_dir: temp_dir,
            }
        }
    }

    #[test_context(IntegrationTestContext)]
    #[test]
    fn test_complete_work_session_workflow(_ctx: &mut IntegrationTestContext) {
        // 1. Initialize database
        let _db = Db::new().unwrap();
        
        // 2. Create workdays and tasks components
        let mut workdays = Workdays::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // 3. Create a workday
        let today = Utc::now().date_naive();
        workdays.insert_start(today).unwrap();
        
        // 4. Create some tasks for the day
        let task1 = Task::new("Morning standup", "Daily team meeting", Some(25));
        let task2 = Task::new("Code review", "Review PR #123", Some(50));
        let task3 = Task::new("Bug fix", "Fix login issue", Some(75));
        
        let _insert1 = tasks.insert(&task1);
        let _insert2 = tasks.insert(&task2);
        let _insert3 = tasks.insert(&task3);
        
        // 5. Complete some tasks during the day
        let task_list = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        assert_eq!(task_list.len(), 3);
        
        // Mark first two tasks as complete
        let mut completed_task1 = task_list[0].clone();
        let mut completed_task2 = task_list[1].clone();
        
        completed_task1.completeness = Some(100);
        completed_task2.completeness = Some(100);
        
        let _update1 = tasks.update(&completed_task1);
        let _update2 = tasks.update(&completed_task2);
        
        // 6. End the workday
        workdays.insert_end(today).unwrap();
        
        // 7. Verify the workflow completed successfully
        let final_workday = workdays.fetch(today).unwrap();
        assert!(final_workday.is_some());
        assert!(final_workday.unwrap().end.is_some());
        
        let final_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        let completed_tasks: Vec<_> = final_tasks.iter().filter(|t| t.completeness == Some(100)).collect();
        assert_eq!(completed_tasks.len(), 2); // task1 and task2 completed
    }

    #[test_context(IntegrationTestContext)]
    #[test]
    fn test_multi_day_workflow(_ctx: &mut IntegrationTestContext) {
        let _db = Db::new().unwrap();
        let mut workdays = Workdays::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        let base_date = Utc::now() - Duration::days(3);
        
        // Create workdays for 3 consecutive days
        for day in 0..3 {
            let day_date = (base_date + Duration::days(day)).date_naive();
            
            workdays.insert_start(day_date).unwrap();
            
            // Create task for each day
            let task_name = format!("Day {} Task", day + 1);
            let task = Task::new(&task_name, "Daily task", Some(50));
            let _insert = tasks.insert(&task);
            
            // Complete the task
            let task_list = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
            let mut last_task = task_list.last().unwrap().clone();
            last_task.completeness = Some(100);
            let _update = tasks.update(&last_task);
            
            workdays.insert_end(day_date).unwrap();
        }
        
        // Verify all tasks were created
        let all_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        assert!(all_tasks.len() >= 3);
        
        let completed_tasks: Vec<_> = all_tasks.iter().filter(|t| t.completeness == Some(100)).collect();
        assert!(completed_tasks.len() >= 3);
    }

    #[test_context(IntegrationTestContext)]
    #[test]
    fn test_task_lifecycle_management(_ctx: &mut IntegrationTestContext) {
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Create tasks with different completion levels
        let task1 = Task::new("New Task", "Just created", Some(0));
        let task2 = Task::new("In Progress Task", "Half done", Some(50));
        let task3 = Task::new("Almost Done Task", "Nearly finished", Some(90));
        
        let _insert1 = tasks.insert(&task1);
        let _insert2 = tasks.insert(&task2);
        let _insert3 = tasks.insert(&task3);
        
        // Get all tasks
        let all_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        assert_eq!(all_tasks.len(), 3);
        
        // Complete the first task
        let mut task_to_complete = all_tasks[0].clone();
        task_to_complete.completeness = Some(100);
        let update_result = tasks.update(&task_to_complete);
        assert!(update_result.is_ok());
        
        // Verify completion
        let updated_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        let completed_count = updated_tasks.iter().filter(|t| t.completeness == Some(100)).count();
        assert_eq!(completed_count, 1);
    }

    #[test_context(IntegrationTestContext)]
    #[test]
    fn test_workday_time_management(_ctx: &mut IntegrationTestContext) {
        let _db = Db::new().unwrap();
        let mut workdays = Workdays::new().unwrap();
        
        // Test creating workdays for different dates
        let today = Utc::now().date_naive();
        let yesterday = (Utc::now() - Duration::days(1)).date_naive();
        
        // Create workdays
        workdays.insert_start(yesterday).unwrap();
        workdays.insert_start(today).unwrap();
        
        // End workdays
        workdays.insert_end(yesterday).unwrap();
        workdays.insert_end(today).unwrap();
        
        // Verify both workdays exist and are completed
        let yesterday_workday = workdays.fetch(yesterday).unwrap();
        let today_workday = workdays.fetch(today).unwrap();
        
        assert!(yesterday_workday.is_some());
        assert!(today_workday.is_some());
        assert!(yesterday_workday.unwrap().end.is_some());
        assert!(today_workday.unwrap().end.is_some());
    }

    #[test_context(IntegrationTestContext)]
    #[test]
    fn test_error_handling_and_recovery(_ctx: &mut IntegrationTestContext) {
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Test creating a valid task
        let task = Task::new("Valid task", "This should work", Some(50));
        let result = tasks.insert(&task);
        assert!(result.is_ok());
        
        // Test operations on non-existent IDs
        let non_existent_task = tasks.get_by_id(99999).unwrap();
        assert!(non_existent_task.is_none());
        
        // Test that valid operations still work after error cases
        let another_task = Task::new("Another task", "Should still work", Some(75));
        let result2 = tasks.insert(&another_task);
        assert!(result2.is_ok());
        
        let all_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        assert_eq!(all_tasks.len(), 2);
    }

    #[test_context(IntegrationTestContext)]
    #[test]
    fn test_configuration_integration(_ctx: &mut IntegrationTestContext) {
        // Test that configuration works with database operations
        let config = Config::default();
        let save_result = config.save();
        assert!(save_result.is_ok());
        
        // Initialize database after config
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Database operations should work with config present
        let task = Task::new("Config Test", "Testing with config", Some(25));
        let result = tasks.insert(&task);
        assert!(result.is_ok());
        
        let loaded_config = Config::read();
        assert!(loaded_config.is_ok());
    }
}