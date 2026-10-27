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
        undo_the_source_rename(&conn);
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
            "INSERT INTO inbox (issue_key, issue_id, summary, priority_rank, url, first_seen, last_seen, notified, pinned, dismissed) VALUES ('KA-1', '1', 's', 3, 'u', datetime('now'), datetime('now'), 0, 0, 0)",
            [],
        )
        .unwrap();
        assert!(
            conn.execute("UPDATE inbox SET left_reason = 'maybe' WHERE issue_key = 'KA-1'", []).is_err(),
            "a third reason must be unwritable"
        );
    }

    /// Undoes migration 19 by hand, since `rollback_to` only forgets the record.
    fn undo_the_source_rename(conn: &rusqlite::Connection) {
        for statement in [
            "DROP INDEX idx_inbox_source",
            "DROP INDEX idx_inbox_active",
            "DROP INDEX idx_inbox_sort",
            "DROP INDEX idx_inbox_snoozed",
            "DROP INDEX idx_tasks_issue_key",
            "ALTER TABLE inbox DROP COLUMN source",
            "ALTER TABLE inbox RENAME TO jira_inbox",
            "ALTER TABLE inbox_statuses RENAME TO jira_statuses",
            "ALTER TABLE inbox_priorities RENAME TO jira_priorities",
            "ALTER TABLE tasks RENAME COLUMN issue_key TO jira_key",
            "CREATE INDEX idx_jira_inbox_active ON jira_inbox(dismissed, pinned DESC, priority_rank ASC, last_seen DESC)",
            "CREATE INDEX idx_jira_inbox_sort ON jira_inbox(dismissed, pinned DESC, sort_value DESC, priority_rank ASC)",
            "CREATE INDEX idx_jira_inbox_snoozed ON jira_inbox(snoozed_until)",
            "CREATE INDEX idx_tasks_jira_key ON tasks(jira_key)",
        ] {
            conn.execute(statement, []).unwrap();
        }
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
        undo_the_source_rename(&conn);
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
            .prepare("SELECT issue_key, project, issue_type FROM inbox ORDER BY issue_key")
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
            .prepare("SELECT name, rank FROM inbox_priorities")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(priorities, vec![("High".to_string(), 2)], "the vocabulary starts with the names the rows carry");
    }

    /// Migration 19 keeps every row and every taken task, and says they are Jira's.
    ///
    /// The rename is the whole upgrade for an inbox that has only ever had
    /// one source: a row or a task's link lost here would be an issue the
    /// person had in hand and no longer sees.
    #[test_context(MigrationTestContext)]
    #[serial]
    #[test]
    fn the_rename_keeps_every_issue_and_says_it_came_from_jira(_ctx: &mut MigrationTestContext) {
        let mut conn = Db::new_without_migrations().unwrap();
        let manager = MigrationManager::new();
        manager.run_migrations(&mut conn).unwrap();

        manager.rollback_to(&mut conn, 18).unwrap();
        undo_the_source_rename(&conn);
        conn.execute(
            "INSERT INTO jira_inbox (issue_key, issue_id, summary, priority, priority_rank, url, first_seen, last_seen, notified, pinned, dismissed, status_id)
             VALUES ('PAY-12', '1', 's', 'High', 2, 'u', datetime('now'), datetime('now'), 0, 1, 0, '3')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO jira_statuses (id, name) VALUES ('3', 'In Progress')", []).unwrap();
        conn.execute("INSERT INTO jira_priorities (name, rank) VALUES ('High', 2)", []).unwrap();
        conn.execute(
            "INSERT INTO tasks (id, task_id, name, comment, completeness, jira_key) VALUES (5, 5, 'PAY-12 s', '', 0, 'PAY-12')",
            [],
        )
        .unwrap();

        manager.run_migrations(&mut conn).unwrap();
        let row: (String, String, i32, String) = conn
            .query_row(
                "SELECT i.issue_key, i.source, i.pinned, s.name FROM inbox i JOIN inbox_statuses s ON s.id = i.status_id",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(row, ("PAY-12".to_string(), "jira".to_string(), 1, "In Progress".to_string()));
        let rank: i32 = conn
            .query_row("SELECT rank FROM inbox_priorities WHERE name = 'High'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rank, 2);
        let linked: String = conn.query_row("SELECT issue_key FROM tasks WHERE id = 5", [], |row| row.get(0)).unwrap();
        assert_eq!(linked, "PAY-12", "the task still knows the issue it was taken from");
    }
}
