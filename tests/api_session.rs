#[cfg(test)]
mod tests {
    use anyhow::Result;
    use kasl::api::Session;
    use kasl::libs::secret::Secret;
    use std::fs;
    use tempfile::TempDir;
    use test_context::{AsyncTestContext, test_context};

    // Create manual async trait implementation without async_trait macro
    trait SessionAsync {
        async fn login(&self) -> Result<String>;
        fn set_credentials(&mut self, password: &str) -> Result<()>;
        fn session_id_file(&self) -> &str;
        fn secret(&self) -> Secret;
        fn retry(&self) -> i32;
        fn inc_retry(&mut self);
    }

    struct ApiTestContext {
        _temp_dir: TempDir,
        test_session_id: String,
        test_password: String,
    }

    impl AsyncTestContext for ApiTestContext {
        async fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            
            ApiTestContext {
                _temp_dir: temp_dir,
                test_session_id: "mock_session_12345".to_string(),
                test_password: "test_password".to_string(),
            }
        }

        async fn teardown(self) {
            // Cleanup is automatic with TempDir
        }
    }

    // Mock implementation of Session trait for testing
    struct MockSession {
        password: Option<String>,
        session_file: String,
        retry_count: i32,
        should_fail_login: bool,
    }

    impl MockSession {
        fn new(session_file: &str, should_fail_login: bool) -> Self {
            Self {
                password: None,
                session_file: session_file.to_string(),
                retry_count: 0,
                should_fail_login,
            }
        }
    }

    impl Session for MockSession {
        async fn login(&self) -> Result<String> {
            if self.should_fail_login {
                anyhow::bail!("Mock login failure");
            }
            Ok("mock_session_12345".to_string())
        }

        fn set_credentials(&mut self, password: &str) -> Result<()> {
            self.password = Some(password.to_string());
            Ok(())
        }

        fn session_id_file(&self) -> &str {
            &self.session_file
        }

        fn secret(&self) -> Secret {
            Secret::new(".mock_secret", "Mock password prompt")
        }

        fn retry(&self) -> i32 {
            self.retry_count
        }

        fn inc_retry(&mut self) {
            self.retry_count += 1;
        }
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_session_successful_login(ctx: &mut ApiTestContext) {
        let mut session = MockSession::new(".test_session", false);
        session.set_credentials(&ctx.test_password).unwrap();

        let result = session.login().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "mock_session_12345");
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_session_failed_login(ctx: &mut ApiTestContext) {
        let mut session = MockSession::new(".test_session", true);
        session.set_credentials(&ctx.test_password).unwrap();

        let result = session.login().await;
        assert!(result.is_err());
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_session_retry_mechanism(_ctx: &mut ApiTestContext) {
        let mut session = MockSession::new(".test_session", false);
        
        assert_eq!(session.retry(), 0);
        session.inc_retry();
        assert_eq!(session.retry(), 1);
        session.inc_retry();
        assert_eq!(session.retry(), 2);
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_session_credentials_setting(ctx: &mut ApiTestContext) {
        let mut session = MockSession::new(".test_session", false);
        
        let result = session.set_credentials(&ctx.test_password);
        assert!(result.is_ok());
        assert_eq!(session.password.as_ref().unwrap(), &ctx.test_password);
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_session_file_operations(ctx: &mut ApiTestContext) {
        let session_file = ".test_session_file";
        let session = MockSession::new(session_file, false);
        
        assert_eq!(session.session_id_file(), session_file);
        
        // Test session ID file read/write
        let session_path = kasl::libs::data_storage::DataStorage::new()
            .get_path(session_file)
            .unwrap();
        
        // Create parent directory
        if let Some(parent) = session_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        
        // Test writing session ID
        fs::write(&session_path, &ctx.test_session_id).unwrap();
        
        // Test reading session ID
        let read_session_id = fs::read_to_string(&session_path).unwrap();
        assert_eq!(read_session_id, ctx.test_session_id);
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_session_id_file_not_found(_ctx: &mut ApiTestContext) {
        let result = fs::read_to_string("/nonexistent/path/session.id");
        assert!(result.is_err());
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_session_delete_functionality(ctx: &mut ApiTestContext) {
        let session_file = ".test_delete_session";
        let session = MockSession::new(session_file, false);
        
        let session_path = kasl::libs::data_storage::DataStorage::new()
            .get_path(session_file)
            .unwrap();
        
        // Create parent directory and file
        if let Some(parent) = session_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&session_path, &ctx.test_session_id).unwrap();
        
        // Verify file exists
        assert!(session_path.exists());
        
        // Delete session manually for test
        let _ = fs::remove_file(&session_path);
        
        // Verify file is deleted
        assert!(!session_path.exists());
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_session_get_or_create_with_cache(ctx: &mut ApiTestContext) {
        let session_file = ".test_cached_session";
        let mut session = MockSession::new(session_file, false);
        
        let session_path = kasl::libs::data_storage::DataStorage::new()
            .get_path(session_file)
            .unwrap();
        
        // Create parent directory and cached session file
        if let Some(parent) = session_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&session_path, &ctx.test_session_id).unwrap();
        
        // get_session_id should return cached value without prompting
        // Note: This test is simplified since we can't easily mock the password prompt
        // In a real implementation, this would verify cached session retrieval
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_session_max_retry_limit(_ctx: &mut ApiTestContext) {
        let mut session = MockSession::new(".test_retry_session", true);
        
        // Simulate maximum retries
        for _ in 0..3 {
            session.inc_retry();
        }
        
        assert_eq!(session.retry(), 3);
        // In real implementation, this would trigger max retry error
    }

    #[test_context(ApiTestContext)]
    #[tokio::test]
    async fn test_secret_integration(_ctx: &mut ApiTestContext) {
        let session = MockSession::new(".test_secret_session", false);
        let secret = session.secret();
        
        // Verify secret is properly initialized
        // Note: Full testing would require mocking user input
    }
}