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

    struct SimpleIntegrationTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for SimpleIntegrationTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            
            SimpleIntegrationTestContext {
                _temp_dir: temp_dir,
            }
        }
    }

    #[test_context(SimpleIntegrationTestContext)]
    #[test]
    fn test_complete_work_session_workflow(_ctx: &mut SimpleIntegrationTestContext) {
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

    #[test_context(SimpleIntegrationTestContext)]
    #[test]
    fn test_multi_day_workflow(_ctx: &mut SimpleIntegrationTestContext) {
        let _db = Db::new().unwrap();
        let mut workdays = Workdays::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        let base_date = Utc::now() - Duration::days(3);
        
        // Create workdays for 3 consecutive days
        for day in 0..3 {
            let day_start = base_date + Duration::days(day) + Duration::hours(9);
            let day_end = day_start + Duration::hours(8);
            
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
            
            // End the workday
            workdays.insert_end(day_date).unwrap();
        }
        
        // Verify all tasks were created
        let all_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        assert!(all_tasks.len() >= 3);
        
        let completed_tasks: Vec<_> = all_tasks.iter().filter(|t| t.completeness == Some(100)).collect();
        assert!(completed_tasks.len() >= 3);
    }

    #[test_context(SimpleIntegrationTestContext)]
    #[test]
    fn test_configuration_integration(_ctx: &mut SimpleIntegrationTestContext) {
        // 1. Create and save a configuration
        let config = Config {
            monitor: Some(kasl::libs::config::MonitorConfig {
                min_pause_duration: 30,
                pause_threshold: 60,
                poll_interval: 1000,
                activity_threshold: 30,
                min_work_interval: 10,
            }),
            server: Some(kasl::libs::config::ServerConfig {
                api_url: "https://test.example.com".to_string(),
                auth_token: "test_token_123".to_string(),
            }),
            si: None,
            gitlab: None,
            jira: None,
        };
        
        let save_result = config.save();
        assert!(save_result.is_ok());
        
        // 2. Read configuration back
        let loaded_config = Config::read().unwrap();
        
        assert!(loaded_config.monitor.is_some());
        assert!(loaded_config.server.is_some());
        
        let monitor_config = loaded_config.monitor.unwrap();
        assert_eq!(monitor_config.min_pause_duration, 30);
        assert_eq!(monitor_config.pause_threshold, 60);
        
        let server_config = loaded_config.server.unwrap();
        assert_eq!(server_config.api_url, "https://test.example.com");
        assert_eq!(server_config.auth_token, "test_token_123");
        
        // 3. Verify configuration integrates with database operations
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Tasks should work with configuration loaded
        let task = Task::new("Config Test Task", "Test with config loaded", Some(60));
        let result = tasks.insert(&task);
        assert!(result.is_ok());
    }

    #[test_context(SimpleIntegrationTestContext)]
    #[test]
    fn test_database_consistency(_ctx: &mut SimpleIntegrationTestContext) {
        // Test that database operations maintain consistency
        let _db = Db::new().unwrap();
        let mut workdays = Workdays::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // Create workday for today
        let today = Utc::now().date_naive();
        workdays.insert_start(today).unwrap();
        
        // Create tasks
        for i in 1..=5 {
            let task = Task::new(&format!("Task {}", i), &format!("Description {}", i), Some(i * 20));
            let _insert = tasks.insert(&task);
        }
        
        // Complete every other task
        let task_list = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        for (index, task) in task_list.iter().enumerate() {
            if index % 2 == 0 {
                let mut completed_task = task.clone();
                completed_task.completeness = Some(100);
                let _update = tasks.update(&completed_task);
            }
        }
        
        // End workday
        workdays.insert_end(today).unwrap();
        
        // Verify consistency
        let final_workday = workdays.fetch(today).unwrap();
        let final_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        
        assert!(final_workday.is_some());
        assert_eq!(final_tasks.len(), 5);
        
        let completed_count = final_tasks.iter().filter(|t| t.completeness == Some(100)).count();
        assert_eq!(completed_count, 3); // Tasks 1, 3, and 5 (indices 0, 2, 4)
    }

    #[test_context(SimpleIntegrationTestContext)]
    #[test]
    fn test_error_recovery_workflow(_ctx: &mut SimpleIntegrationTestContext) {
        let _db = Db::new().unwrap();
        let mut tasks = Tasks::new().unwrap();
        
        // 1. Create valid data first
        let valid_task = Task::new("Valid task", "This should work", Some(50));
        let valid_result = tasks.insert(&valid_task);
        assert!(valid_result.is_ok());
        
        // 2. Test operations with edge cases
        let edge_cases = vec![
            Task::new("", "", Some(0)),           // Empty strings
            Task::new("Very long task name that might exceed some theoretical limit but should still work fine in most database systems", "Also a very long description", Some(100)), // Long strings
            Task::new("Special chars: àáâãäå ñü", "Unicode test: 你好世界", Some(25)), // Unicode
        ];
        
        for edge_task in edge_cases {
            let result = tasks.insert(&edge_task);
            // Results may vary, but operations should not crash
            match result {
                Ok(_) => {
                    // If successful, continue
                },
                Err(_) => {
                    // If error, that's also acceptable for edge cases
                }
            }
        }
        
        // 3. Verify that valid operations still work after edge cases
        let another_valid_task = Task::new("Another valid task", "This should also work", Some(75));
        let another_valid_result = tasks.insert(&another_valid_task);
        assert!(another_valid_result.is_ok());
        
        let all_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        assert!(all_tasks.len() >= 2); // At least the two valid tasks
    }

    #[test_context(SimpleIntegrationTestContext)]
    #[test]
    fn test_concurrent_access_simulation(_ctx: &mut SimpleIntegrationTestContext) {
        // Simulate concurrent access by creating multiple database handles
        let _db1 = Db::new().unwrap();
        let _db2 = Db::new().unwrap();
        
        let mut tasks1 = Tasks::new().unwrap();
        let mut tasks2 = Tasks::new().unwrap();
        let mut workdays1 = Workdays::new().unwrap();
        let mut workdays2 = Workdays::new().unwrap();
        
        // Create data through first handle
        let task1 = Task::new("From handle 1", "Task via first database handle", Some(30));
        let yesterday = (Utc::now() - Duration::days(1)).date_naive();
        workdays1.insert_start(yesterday).unwrap();
        let _insert1 = tasks1.insert(&task1);
        
        // Create data through second handle
        let task2 = Task::new("From handle 2", "Task via second database handle", Some(70));
        let today = Utc::now().date_naive();
        workdays2.insert_start(today).unwrap();
        let _insert2 = tasks2.insert(&task2);
        
        // Both handles should see all data
        let tasks_via_handle1 = tasks1.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        let tasks_via_handle2 = tasks2.fetch(kasl::libs::task::TaskFilter::All).unwrap();
        
        assert_eq!(tasks_via_handle1.len(), tasks_via_handle2.len());
        assert!(tasks_via_handle1.len() >= 2);
        
        // Clean up workdays
        workdays1.insert_end(yesterday).unwrap();
        workdays2.insert_end(today).unwrap();
    }

    #[test_context(SimpleIntegrationTestContext)]
    #[test]
    fn test_data_persistence_across_sessions(_ctx: &mut SimpleIntegrationTestContext) {
        // First session: create data
        {
            let _db = Db::new().unwrap();
            let mut tasks = Tasks::new().unwrap();
            let mut workdays = Workdays::new().unwrap();
            
            let task = Task::new("Persistent Task", "Should survive across sessions", Some(40));
            let today = Utc::now().date_naive();
            workdays.insert_start(today).unwrap();
            let _insert = tasks.insert(&task);
            workdays.insert_end(today).unwrap();
        } // Database connection closes here
        
        // Second session: verify data persists
        {
            let _db = Db::new().unwrap();
            let mut tasks = Tasks::new().unwrap();
            let mut workdays = Workdays::new().unwrap();
            
            let persisted_tasks = tasks.fetch(kasl::libs::task::TaskFilter::All).unwrap();
            let today = Utc::now().date_naive();
            let persisted_workday = workdays.fetch(today).unwrap();
            
            assert!(!persisted_tasks.is_empty());
            assert!(persisted_workday.is_some());
            
            let persistent_task = persisted_tasks.iter()
                .find(|t| t.name == "Persistent Task");
            assert!(persistent_task.is_some());
        }
    }
}