#[cfg(test)]
mod tests {
    use kasl::db::db::Db;
    use kasl::db::migrations::{MigrationManager, get_db_version, needs_migration};
    use serial_test::serial;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    struct MigrationTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for MigrationTestContext {
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
            MigrationTestContext { _temp_dir: temp_dir }
        }
    }

    #[test_context(MigrationTestContext)]
    #[serial]
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
    #[serial]
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
        for (i, entry) in history.iter().enumerate() {
            assert_eq!(entry.0 as usize, i + 1);
        }
    }

    #[test_context(MigrationTestContext)]
    #[serial]
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

    /// Migration 17 gives every task left on chain 0 a chain of its own.
    ///
    /// Before it, every task not made by `task add` sat on chain 0, and the
    /// incomplete-task query - which groups by chain - saw them as one task.
    #[test_context(MigrationTestContext)]
    #[serial]
    #[test]
    fn tasks_on_chain_zero_get_chains_of_their_own(_ctx: &mut MigrationTestContext) {
        let mut conn = Db::new_without_migrations().unwrap();
        let manager = MigrationManager::new();
        manager.run_migrations(&mut conn).unwrap();

        // Back to the schema as version 16 left it, with two old-style rows.
        manager.rollback_to(&mut conn, 16).unwrap();
        drop_inbox_rule_columns(&conn);
        conn.execute("ALTER TABLE jira_inbox DROP COLUMN left_reason", []).unwrap();
        conn.execute("ALTER TABLE jira_inbox DROP COLUMN resolution", []).unwrap();
        conn.execute(
            "INSERT INTO tasks (id, task_id, name, comment, completeness) VALUES (7, 0, 'KA-1 taken', '', 0), (9, 0, 'imported', '', 100), (12, 7, 'KA-1 taken', '', 50)",
            [],
        )
        .unwrap();

        manager.run_migrations(&mut conn).unwrap();
        let chains: Vec<(i32, i32)> = conn
            .prepare("SELECT id, task_id FROM tasks ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            chains,
            vec![(7, 7), (9, 9), (12, 7)],
            "chain 0 rows become their own chain; a real chain is kept"
        );

        // And the departure reason only takes the two answers there are.
        conn.execute(
            "INSERT INTO jira_inbox (issue_key, issue_id, summary, priority_rank, url, first_seen, last_seen, notified, pinned, dismissed) VALUES ('KA-1', '1', 's', 3, 'u', datetime('now'), datetime('now'), 0, 0, 0)",
            [],
        )
        .unwrap();
        assert!(
            conn.execute("UPDATE jira_inbox SET left_reason = 'maybe' WHERE issue_key = 'KA-1'", [])
                .is_err(),
            "a third reason must be unwritable"
        );
    }

    /// Undoes migration 18 by hand, since `rollback_to` only forgets the record.
    fn drop_inbox_rule_columns(conn: &rusqlite::Connection) {
        for column in ["issue_type", "project", "ruled_by"] {
            conn.execute(&format!("ALTER TABLE jira_inbox DROP COLUMN {column}"), []).unwrap();
        }
        conn.execute("DROP TABLE jira_priorities", []).unwrap();
    }

    /// Migration 18 gives existing rows the project their key names.
    ///
    /// A project rule must act on the inbox as it stands after the upgrade,
    /// not only on issues the next poll happens to bring.
    #[test_context(MigrationTestContext)]
    #[serial]
    #[test]
    fn existing_issues_get_the_project_their_key_names(_ctx: &mut MigrationTestContext) {
        let mut conn = Db::new_without_migrations().unwrap();
        let manager = MigrationManager::new();
        manager.run_migrations(&mut conn).unwrap();

        manager.rollback_to(&mut conn, 17).unwrap();
        drop_inbox_rule_columns(&conn);
        conn.execute(
            "INSERT INTO jira_inbox (issue_key, issue_id, summary, priority, priority_rank, url, first_seen, last_seen, notified, pinned, dismissed)
             VALUES ('PAY-12', '1', 's', 'High', 2, 'u', datetime('now'), datetime('now'), 0, 0, 0),
                    ('odd', '2', 's', NULL, 999, 'u', datetime('now'), datetime('now'), 0, 0, 0)",
            [],
        )
        .unwrap();

        manager.run_migrations(&mut conn).unwrap();
        let rows: Vec<(String, Option<String>, Option<String>)> = conn
            .prepare("SELECT issue_key, project, issue_type FROM jira_inbox ORDER BY issue_key")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![("PAY-12".to_string(), Some("PAY".to_string()), None), ("odd".to_string(), None, None)],
            "a key's prefix is its project; a key without one is left alone, and the type waits for Jira"
        );

        let priorities: Vec<(String, i32)> = conn
            .prepare("SELECT name, rank FROM jira_priorities")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(priorities, vec![("High".to_string(), 2)], "the vocabulary starts with the names the rows carry");
    }
}
