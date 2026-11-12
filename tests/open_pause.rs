//! The pause the running watcher has open, told apart from one an earlier
//! watcher left open when it was killed mid-pause - a row that keeps
//! `end IS NULL` for good, and must not read as a break happening now.

use chrono::{Duration, Local, Timelike};
use kasl::db::pauses::Pauses;
use serial_test::serial;
use tempfile::TempDir;

fn sandbox() -> TempDir {
    let dir = TempDir::new().unwrap();
    // SAFETY: tests touching the env are #[serial]
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("LOCALAPPDATA", dir.path());
    }
    dir
}

#[test]
#[serial]
fn only_the_running_watchers_pause_is_open() {
    let _dir = sandbox();
    let pauses = Pauses::new().unwrap();
    let now = Local::now().naive_local().with_nanosecond(0).unwrap();
    let watcher_started = now - Duration::hours(1);

    pauses.insert_start_with_time(now - Duration::hours(3)).unwrap();
    assert_eq!(
        pauses.open_since(watcher_started).unwrap(),
        None,
        "a pause left open by an earlier watcher is not a break now"
    );

    let started = now - Duration::minutes(12);
    pauses.insert_start_with_time(started).unwrap();
    assert_eq!(pauses.open_since(watcher_started).unwrap(), Some(started));

    pauses.insert_end().unwrap();
    assert_eq!(pauses.open_since(watcher_started).unwrap(), None, "a closed pause is over");
}

#[test]
#[serial]
fn a_pause_stamped_in_the_watchers_first_second_is_its_own() {
    // Stamps are whole seconds and the PID file's time is not: a pause
    // stamped 10:00:00 belongs to a watcher that started at 10:00:00.7.
    let _dir = sandbox();
    let pauses = Pauses::new().unwrap();
    let second = Local::now().naive_local().with_nanosecond(0).unwrap() - Duration::minutes(5);
    pauses.insert_start_with_time(second).unwrap();

    let watcher_started = second + Duration::milliseconds(700);
    assert_eq!(pauses.open_since(watcher_started).unwrap(), Some(second));
}
