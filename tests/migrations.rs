#[cfg(test)]
mod tests {
    use kasl::db::db::Db;
    use kasl::db::migrations::{get_db_version, needs_migration, MigrationManager};
    use tempfile::TempDir;
    use test_context::{test_context, TestContext};

    struct MigrationTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for MigrationTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            std::env::set_var("HOME", temp_dir.path());
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
            MigrationTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(MigrationTestContext)]
    #[test]
    fn test_migrations_run_automatically(_ctx: &mut MigrationTestContext) {
        // Create new DB which should run all migrations
        let db = Db::new().unwrap();

        // Check that migrations were applied
        let version = get_db_version(&db.conn).unwrap();
        assert!(version > 0);

        // Check that no more migrations are needed
        assert!(!needs_migration(&db.conn).unwrap());
    }

    #[test_context(MigrationTestContext)]
    #[test]
    fn test_migration_history(_ctx: &mut MigrationTestContext) {
        let mut conn = Db::new_without_migrations().unwrap();
        let manager = MigrationManager::new();

        // Run migrations
        manager.run_migrations(&mut conn).unwrap();

        // Get history
        let history = manager.get_migration_history(&conn).unwrap();
        assert!(!history.is_empty());

        // Verify migrations are recorded in order
        for i in 0..history.len() {
            assert_eq!(history[i].0 as usize, i + 1);
        }
    }

    #[test_context(MigrationTestContext)]
    #[test]
    fn test_migration_idempotency(_ctx: &mut MigrationTestContext) {
        let mut conn = Db::new_without_migrations().unwrap();
        let manager = MigrationManager::new();

        // Run migrations twice
        manager.run_migrations(&mut conn).unwrap();
        let version1 = get_db_version(&conn).unwrap();

        manager.run_migrations(&mut conn).unwrap();
        let version2 = get_db_version(&conn).unwrap();

        // Version should not change
        assert_eq!(version1, version2);
    }
}
