//! `kasl undo`: what a remove command took away comes back.
//!
//! What is under test is the promise `--yes` now leans on: a removal is one
//! operation that returns whole - the rows, their ids, the tags that went
//! with them - and a removal that cannot return cleanly says so instead of
//! writing over what was added since.

#[cfg(test)]
mod tests {
    use chrono::{Local, NaiveDateTime, TimeDelta};
    use kasl::db::pauses::Pauses;
    use kasl::db::tags::{Tag, Tags};
    use kasl::db::tasks::Tasks;
    use kasl::db::templates::{TaskTemplate, Templates};
    use kasl::db::trash::{Kind, Trash};
    use kasl::db::workdays::Workdays;
    use kasl::libs::data_storage::DataStorage;
    use kasl::libs::task::{Task, TaskFilter};
    use serde_json::Value;
    use serial_test::serial;
    use std::path::Path;
    use std::process::{Command, Output, Stdio};
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    struct UndoTestContext {
        temp_dir: TempDir,
    }

    impl TestContext for UndoTestContext {
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
            UndoTestContext { temp_dir }
        }
    }

    fn add_task(tasks: &mut Tasks, name: &str) -> i32 {
        tasks.insert(&Task::new(name, "", Some(40))).unwrap().get().unwrap()[0].id.unwrap()
    }

    fn names(tasks: &mut Tasks) -> Vec<String> {
        tasks.fetch(TaskFilter::All).unwrap().into_iter().map(|task| task.name).collect()
    }

    fn tag_names(tags: &mut Tags, task: i32) -> Vec<String> {
        tags.get_tags_by_task(task).unwrap().into_iter().map(|tag| tag.name).collect()
    }

    fn kasl(dir: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kasl"))
            .args(args)
            .env("HOME", dir)
            .env("LOCALAPPDATA", dir)
            .stdin(Stdio::null())
            .output()
            .expect("kasl runs")
    }

    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn tasks_removed_by_one_command_come_back_together(_ctx: &mut UndoTestContext) {
        let mut tasks = Tasks::new().unwrap();
        let first = add_task(&mut tasks, "Write the report");
        let second = add_task(&mut tasks, "Review the release");
        let kept = add_task(&mut tasks, "Stay");

        assert_eq!(tasks.delete_many(&[first, second]).unwrap(), 2);
        assert_eq!(names(&mut tasks), ["Stay"]);

        let removal = Trash::new().unwrap().restore_last().unwrap().expect("a removal to undo");

        assert_eq!(removal.kind, Kind::Task);
        let restored: Vec<(i64, Option<i64>)> = removal.items.iter().map(|item| (item.id, item.renumbered_from)).collect();
        assert_eq!(restored, [(first as i64, None), (second as i64, None)], "each task returns under its own id");
        assert_eq!(removal.items[0].date, Some(Local::now().date_naive()));

        let back = tasks.fetch(TaskFilter::ByIds(vec![first, second, kept])).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[0].name, "Write the report");
        assert_eq!(back[0].completeness, Some(40));
        assert_eq!(back[0].task_id, Some(first), "the chain is the one it was");

        assert!(Trash::new().unwrap().restore_last().unwrap().is_none(), "a removal is undone once");
    }

    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn undo_goes_back_one_removal_at_a_time(_ctx: &mut UndoTestContext) {
        let mut tasks = Tasks::new().unwrap();
        let first = add_task(&mut tasks, "Removed first");
        let second = add_task(&mut tasks, "Removed second");
        tasks.delete(first).unwrap();
        tasks.delete(second).unwrap();

        let listed = Trash::new().unwrap().list().unwrap();
        let labels: Vec<&str> = listed.iter().map(|removal| removal.items[0].label.as_str()).collect();
        assert_eq!(labels, ["Removed second", "Removed first"], "the latest removal is listed first");

        let mut trash = Trash::new().unwrap();
        assert_eq!(trash.restore_last().unwrap().unwrap().items[0].label, "Removed second");
        assert_eq!(names(&mut tasks), ["Removed second"]);
        assert_eq!(trash.restore_last().unwrap().unwrap().items[0].label, "Removed first");
        assert_eq!(names(&mut tasks), ["Removed first", "Removed second"]);
    }

    /// Deleting the last row of a task takes its tags along (the trigger of
    /// migration 20); an undo that returned the row alone would return an
    /// untagged task.
    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_task_comes_back_with_its_tags(_ctx: &mut UndoTestContext) {
        let mut tasks = Tasks::new().unwrap();
        let mut tags = Tags::new().unwrap();
        let task = add_task(&mut tasks, "Tagged work");
        let ids = tags.get_or_create_tags(&["payments".to_string(), "urgent".to_string()]).unwrap();
        tags.set_task_tags(task, &ids).unwrap();

        tasks.delete(task).unwrap();
        assert_eq!(tags.count_tasks_with_tag(ids[0]).unwrap(), 0, "the tags left with the task");

        Trash::new().unwrap().restore_last().unwrap().unwrap();

        assert_eq!(tag_names(&mut tags, task), ["payments", "urgent"]);
    }

    /// A tag removed from a chain that lives on keeps no link in the trash,
    /// so undoing a row's removal does not bring back a tag taken off since.
    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_row_of_a_living_chain_does_not_bring_back_a_tag_taken_off_since(_ctx: &mut UndoTestContext) {
        let mut tasks = Tasks::new().unwrap();
        let mut tags = Tags::new().unwrap();
        let first = add_task(&mut tasks, "Day one");
        let mut continued = Task::new("Day two", "", Some(80));
        continued.task_id = Some(first);
        let second = tasks.insert(&continued).unwrap().get().unwrap()[0].id.unwrap();
        let ids = tags.get_or_create_tags(&["payments".to_string()]).unwrap();
        tags.set_task_tags(first, &ids).unwrap();

        tasks.delete(second).unwrap();
        tags.remove_tag_from_task(first, ids[0]).unwrap();
        Trash::new().unwrap().restore_last().unwrap().unwrap();

        assert!(tag_names(&mut tags, second).is_empty(), "the tag was taken off the task on purpose");
    }

    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_tag_comes_back_onto_every_task_that_carried_it(_ctx: &mut UndoTestContext) {
        let mut tasks = Tasks::new().unwrap();
        let mut tags = Tags::new().unwrap();
        let first = add_task(&mut tasks, "First");
        let second = add_task(&mut tasks, "Second");
        let tag = tags.create(&Tag::new("payments".to_string(), Some("green".to_string()))).unwrap();
        tags.add_tag_to_task(first, tag).unwrap();
        tags.add_tag_to_task(second, tag).unwrap();

        tags.delete(tag).unwrap();
        assert!(tags.get_by_name("payments").unwrap().is_none());

        let removal = Trash::new().unwrap().restore_last().unwrap().unwrap();

        assert_eq!(removal.kind, Kind::Tag);
        assert_eq!(removal.items[0].label, "payments");
        let back = tags.get_by_name("payments").unwrap().expect("the tag is back");
        assert_eq!((back.id, back.color.as_deref()), (Some(tag), Some("green")));
        assert_eq!(tags.get_tasks_by_tag(tag).unwrap(), [first, second]);
    }

    /// SQLite hands the highest id out again once its row is gone, so a task
    /// added after a removal can sit on a removed task's id. The newer task
    /// keeps it; the returning one says which id it has now.
    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_task_whose_id_was_taken_since_comes_back_under_a_new_one(_ctx: &mut UndoTestContext) {
        let mut tasks = Tasks::new().unwrap();
        let mut tags = Tags::new().unwrap();
        let removed = add_task(&mut tasks, "Removed");
        let ids = tags.get_or_create_tags(&["payments".to_string()]).unwrap();
        tags.set_task_tags(removed, &ids).unwrap();
        tasks.delete(removed).unwrap();

        let newer = add_task(&mut tasks, "Added since");
        assert_eq!(newer, removed, "the premise: the id was handed out again");

        let removal = Trash::new().unwrap().restore_last().unwrap().unwrap();

        let item = &removal.items[0];
        assert_eq!(item.renumbered_from, Some(removed as i64));
        assert_ne!(item.id, removed as i64);
        let back = tasks.get_by_id(item.id as i32).unwrap().unwrap();
        assert_eq!(back.name, "Removed");
        assert_eq!(back.task_id, back.id, "it heads its own chain, not the newer task's");
        assert_eq!(tag_names(&mut tags, item.id as i32), ["payments"], "its tags followed it");

        let untouched = tasks.get_by_id(newer).unwrap().unwrap();
        assert_eq!(untouched.name, "Added since");
        assert!(tag_names(&mut tags, newer).is_empty(), "the newer task did not inherit them");
    }

    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_tag_whose_name_is_in_use_again_is_not_put_back_over_it(_ctx: &mut UndoTestContext) {
        let mut tags = Tags::new().unwrap();
        let old = tags.create(&Tag::new("payments".to_string(), Some("green".to_string()))).unwrap();
        tags.delete(old).unwrap();
        tags.create(&Tag::new("payments".to_string(), Some("red".to_string()))).unwrap();

        let refused = Trash::new().unwrap().restore_last().unwrap_err().to_string();

        assert!(refused.contains("a tag named 'payments' exists now"), "{refused}");
        assert_eq!(tags.get_by_name("payments").unwrap().unwrap().color.as_deref(), Some("red"));
        assert_eq!(
            Trash::new().unwrap().list().unwrap().len(),
            1,
            "the removal is still there to undo once the name is free"
        );
    }

    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_pause_comes_back_as_it_was_recorded(_ctx: &mut UndoTestContext) {
        let pauses = Pauses::new().unwrap();
        let start = NaiveDateTime::parse_from_str("2026-11-10 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let id = pauses.insert_manual(start, TimeDelta::minutes(45), true, Some("dentist")).unwrap() as i32;

        assert_eq!(pauses.delete_many(&[id]).unwrap(), 1);
        assert!(pauses.get_daily_pauses(start.date()).unwrap().is_empty());

        let removal = Trash::new().unwrap().restore_last().unwrap().unwrap();

        assert_eq!(removal.kind, Kind::Pause);
        assert_eq!(removal.items[0].label, "13:00-13:45");
        assert_eq!(removal.items[0].date, Some(start.date()));
        let back = pauses.get_daily_pauses(start.date()).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!((back[0].id, back[0].start, back[0].protected), (id, start, true));
        assert_eq!(back[0].duration, Some(TimeDelta::minutes(45)));
        let reason: String = pauses
            .conn
            .lock()
            .query_row("SELECT reason FROM pauses WHERE id = ?1", [id], |row| row.get(0))
            .unwrap();
        assert_eq!(reason, "dentist");
    }

    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_template_comes_back(_ctx: &mut UndoTestContext) {
        let mut templates = Templates::new().unwrap();
        templates
            .create(&TaskTemplate::new(
                "standup".to_string(),
                "Daily standup".to_string(),
                "Team sync".to_string(),
                100,
            ))
            .unwrap();

        templates.delete("standup").unwrap();
        assert!(!templates.exists("standup").unwrap());

        let removal = Trash::new().unwrap().restore_last().unwrap().unwrap();

        assert_eq!((removal.kind, removal.items[0].label.as_str()), (Kind::Template, "standup"));
        let back = templates.get("standup").unwrap().expect("the template is back");
        assert_eq!(
            (back.task_name.as_str(), back.comment.as_str(), back.completeness),
            ("Daily standup", "Team sync", 100)
        );
    }

    /// The trash is for noticing a mistake, not an archive of everything
    /// ever deleted.
    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_removal_older_than_a_month_is_dropped_by_the_next_one(_ctx: &mut UndoTestContext) {
        let mut tasks = Tasks::new().unwrap();
        let old = add_task(&mut tasks, "Removed long ago");
        let recent = add_task(&mut tasks, "Removed just now");
        tasks.delete(old).unwrap();
        tasks.conn.execute("UPDATE trash SET removed_at = datetime('now', '-31 days')", []).unwrap();

        tasks.delete(recent).unwrap();

        let listed = Trash::new().unwrap().list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].items[0].label, "Removed just now");
    }

    /// `tasks.deleted_at` promised a soft delete nothing ever performed; the
    /// trash replaced the promise, and a removed row is not in its table.
    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_removed_row_is_out_of_its_table(_ctx: &mut UndoTestContext) {
        let mut tasks = Tasks::new().unwrap();
        let task = add_task(&mut tasks, "Gone");
        tasks.delete(task).unwrap();

        let rows: i64 = tasks.conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0)).unwrap();
        assert_eq!(rows, 0);
        let column: bool = tasks
            .conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM pragma_table_info('tasks') WHERE name = 'deleted_at')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!column, "the schema names one way of deleting a task");
    }

    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn the_command_removes_and_puts_back(ctx: &mut UndoTestContext) {
        let dir = ctx.temp_dir.path();
        let added = kasl(dir, &["task", "add", "--name", "Scripted task"]);
        assert!(added.status.success(), "{}", String::from_utf8_lossy(&added.stderr));

        let removed = kasl(dir, &["task", "remove", "1", "--yes", "--plain"]);
        let said = String::from_utf8_lossy(&removed.stdout);
        assert!(said.contains("`kasl undo` puts them back"), "a removal says how to take it back:\n{said}");

        let listed = kasl(dir, &["undo", "--list", "--json"]);
        let listed: Value = serde_json::from_slice(&listed.stdout).expect("one JSON document");
        assert_eq!(listed["kept_days"], 30);
        assert_eq!(listed["removals"][0]["command"], "task remove");
        assert_eq!(listed["removals"][0]["items"][0]["label"], "Scripted task");

        let undone = kasl(dir, &["undo", "--plain"]);
        let said = String::from_utf8_lossy(&undone.stdout);
        assert!(undone.status.success(), "{}", String::from_utf8_lossy(&undone.stderr));
        assert!(said.contains("Put back 1 task that `kasl task remove` removed"), "{said}");
        assert!(said.contains("Scripted task"), "{said}");

        let tasks = kasl(dir, &["task", "list", "--json"]);
        let tasks: Value = serde_json::from_slice(&tasks.stdout).unwrap();
        assert_eq!(tasks["tasks"][0]["name"], "Scripted task");

        let nothing = kasl(dir, &["undo", "--json"]);
        assert!(nothing.status.success(), "nothing to undo is an answer, not a failure");
        let nothing: Value = serde_json::from_slice(&nothing.stdout).unwrap();
        assert!(nothing["restored"].is_null(), "{nothing:#}");
    }

    /// An upload tells the server "these are all the tasks of the day", so a
    /// day pushed between the removal and the undo left the server without
    /// the task. Only sending the day again brings it back there.
    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn a_day_put_back_is_owed_to_the_server_again(ctx: &mut UndoTestContext) {
        let dir = ctx.temp_dir.path();
        let today = Local::now().date_naive();
        let mut tasks = Tasks::new().unwrap();
        let worked = add_task(&mut tasks, "On a day that was worked");
        Workdays::new().unwrap().insert_start(today).unwrap();
        // A task of a date with no workday: there is no day to send for it.
        let stray = add_task(&mut tasks, "On a day that was not");
        tasks
            .conn
            .execute("UPDATE tasks SET timestamp = datetime(timestamp, '-40 days') WHERE id = ?1", [stray])
            .unwrap();
        tasks.delete_many(&[worked, stray]).unwrap();

        std::fs::write(
            DataStorage::new().get_path("config.json").unwrap(),
            r#"{"kasl_server": {"url": "https://kasl.example.com"}}"#,
        )
        .unwrap();

        let undone = kasl(dir, &["undo", "--json"]);
        assert!(undone.status.success(), "{}", String::from_utf8_lossy(&undone.stderr));
        let undone: Value = serde_json::from_slice(&undone.stdout).expect("one JSON document");
        assert_eq!(undone["restored"]["items"].as_array().unwrap().len(), 2, "{undone:#}");
        assert_eq!(undone["queued"], serde_json::json!([today.to_string()]), "{undone:#}");

        let queue = kasl(dir, &["server", "queue", "--json"]);
        let queue: Value = serde_json::from_slice(&queue.stdout).unwrap();
        assert_eq!(queue["days"].as_array().unwrap().len(), 1, "{queue:#}");
        assert_eq!(queue["days"][0]["date"], today.to_string());
    }

    /// A machine that reports to no server owes it nothing: the outbox must
    /// not fill with days nobody will ever collect.
    #[test_context(UndoTestContext)]
    #[serial]
    #[test]
    fn with_no_server_nothing_is_queued(ctx: &mut UndoTestContext) {
        let dir = ctx.temp_dir.path();
        let mut tasks = Tasks::new().unwrap();
        let task = add_task(&mut tasks, "Local only");
        Workdays::new().unwrap().insert_start(Local::now().date_naive()).unwrap();
        tasks.delete(task).unwrap();

        let undone = kasl(dir, &["undo", "--json"]);
        let undone: Value = serde_json::from_slice(&undone.stdout).expect("one JSON document");
        assert_eq!(undone["queued"], serde_json::json!([]), "{undone:#}");

        let queue = kasl(dir, &["server", "queue", "--json"]);
        let queue: Value = serde_json::from_slice(&queue.stdout).unwrap();
        assert_eq!(queue["days"], serde_json::json!([]));
    }
}
