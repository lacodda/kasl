//! Assembling a local day into what kasl-server accepts.
//!
//! The gap this bridges is time. kasl stores instants as bare wall-clock text
//! (`2026-08-31 09:14:00`, no offset), which is unambiguous on one laptop and
//! meaningless once a team spans time zones or a clock shifts for daylight
//! saving. The server refuses a timestamp without an offset for exactly that
//! reason (ADR 0003 in kasl-server), so the offset is attached here, from the
//! machine's own zone, at the instant each timestamp names.
//!
//! Per timestamp, not per day: a day that straddles a daylight-saving change
//! has two offsets in it, and stamping the whole day with one would move half
//! its hours.
//!
//! ```rust,no_run
//! # use kasl::libs::day_upload::build_day_upload;
//! # use chrono::Local;
//! # fn main() -> anyhow::Result<()> {
//! let date = Local::now().date_naive();
//! if let Some(day) = build_day_upload(date)? {
//!     println!("{} has {} pauses", day.date, day.pauses.len());
//! }
//! # Ok(())
//! # }
//! ```

use crate::api::kasl_server::{DayUpload, PauseUpload, TaskUpload};
use crate::db::{
    pauses::Pauses,
    tasks::Tasks,
    workdays::{Workday, Workdays},
};
use crate::libs::config::Config;
use crate::libs::pause::Pause;
use crate::libs::task::{Task, TaskFilter};
use anyhow::{Result, bail};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone};

/// Builds the day for `date`, or `None` when that day was never started.
///
/// A missing day is not an error: `push --last` on a Monday morning after a
/// weekend has nothing to send, and saying so is the honest answer.
///
/// Pauses are taken the same way the daily report takes them (detected
/// pauses above the configured threshold plus every manual one, clipped to
/// the workday), so what reaches the server matches what `kasl report` shows.
/// A dashboard disagreeing with the employee's own terminal would be worse
/// than either being wrong alone.
pub fn build_day_upload(date: NaiveDate) -> Result<Option<DayUpload>> {
    let Some(workday) = Workdays::new()?.fetch(date)? else {
        return Ok(None);
    };

    let monitor_config = Config::read()?.monitor.unwrap_or_default();
    let pauses = Pauses::new()?
        .set_min_duration(monitor_config.min_pause_duration)
        .get_workday_pauses(&workday)?;
    let tasks = Tasks::new()?.fetch(TaskFilter::Date(date))?;

    build_from_parts(&workday, &pauses, &tasks).map(Some)
}

/// The pure half of the assembly: rows in, payload out.
///
/// Split out so the conversion can be tested on days a live database is
/// awkward to hold (one that ends before it starts, one crossing a
/// daylight-saving boundary) without going near the disk.
pub fn build_from_parts(workday: &Workday, pauses: &[Pause], tasks: &[Task]) -> Result<DayUpload> {
    let started_at = with_local_offset(workday.start)?;
    let ended_at = workday.end.map(with_local_offset).transpose()?;

    let pauses = pauses
        .iter()
        .map(|pause| {
            Ok(PauseUpload {
                started_at: with_local_offset(pause.start)?,
                ended_at: pause.end.map(with_local_offset).transpose()?,
                // Seconds as kasl computed them, which is not always
                // `end - start`: neighbouring pauses are merged before they
                // are reported, and the merged span is the honest duration.
                duration_seconds: pause.duration.map(|duration| duration.num_seconds() as i32),
                manual: pause.protected,
                // kasl records a reason for a manual pause but does not read
                // it back out of the database yet, so there is nothing
                // truthful to put here. Sending an empty string would claim a
                // reason was given; the field is left out instead.
                reason: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let tasks = tasks.iter().map(task_upload).collect::<Result<Vec<_>>>()?;

    Ok(DayUpload {
        date: workday.date,
        started_at,
        ended_at,
        pauses,
        tasks,
        // kasl sends every task it holds for the date, so the set is
        // authoritative and a task deleted here is deleted there (ADR 0005).
        tasks_are_complete: true,
    })
}

/// Converts one task, refusing the ones the server would refuse.
///
/// Caught here rather than at the server because a `400` names an index in a
/// payload the user never saw, while this names the task.
fn task_upload(task: &Task) -> Result<TaskUpload> {
    let Some(id) = task.id else {
        // Every task read back from the database has one; a task without an
        // id was never stored, and the server matches re-uploads on this key.
        bail!("task '{}' has no id and cannot be sent", task.name);
    };

    let timestamp = task
        .timestamp
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("task '{}' has no timestamp and cannot be sent", task.name))?;
    let recorded_at = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
        .map_err(|error| anyhow::anyhow!("task '{}' has an unreadable timestamp '{}': {}", task.name, timestamp, error))?;

    Ok(TaskUpload {
        agent_task_id: id,
        agent_group_id: task.task_id,
        recorded_at: with_local_offset(recorded_at)?,
        name: task.name.clone(),
        // An empty comment is kasl's way of saying there is none - the column
        // is not nullable here - so it is sent as absent rather than as an
        // empty string the server would store and show.
        comment: Some(task.comment.clone()).filter(|comment| !comment.trim().is_empty()),
        // Clamped rather than refused: a task that somehow holds 150% is a
        // local data problem, and losing the whole day over it would be worse
        // than filing it as complete. `None` means "not started" here.
        completeness: task.completeness.unwrap_or(0).clamp(0, 100) as i16,
    })
}

/// Attaches this machine's UTC offset to a wall-clock instant.
///
/// The offset comes from the local zone *at that instant*, which is the only
/// way a day spanning a daylight-saving change keeps both its halves right.
///
/// Two wall-clock times have no single offset, and both are refused rather
/// than guessed:
///
/// - The hour skipped when clocks go forward never existed. A stored
///   timestamp inside it is corrupt, and inventing an instant for it would
///   file work at a time nobody worked.
/// - The hour repeated when clocks go back happens twice. Picking one is a
///   coin flip that silently moves an hour of work.
///
/// Both are rare and both are worth a refusal the user can see, because the
/// alternative is a wrong day that looks right.
pub fn with_local_offset(naive: NaiveDateTime) -> Result<DateTime<FixedOffset>> {
    with_offset_of(&Local, naive)
}

/// The same, against a stated zone.
///
/// `with_local_offset` is this with the machine's own zone. Taking the zone as
/// an argument is what makes the two refusals testable at all: the branches
/// exist only where clocks shift, and the machine running the tests may sit
/// in a zone that never does.
fn with_offset_of<Tz: TimeZone>(zone: &Tz, naive: NaiveDateTime) -> Result<DateTime<FixedOffset>> {
    match zone.from_local_datetime(&naive) {
        chrono::LocalResult::Single(local) => Ok(local.fixed_offset()),
        chrono::LocalResult::Ambiguous(_, _) => bail!(
            "'{}' happened twice on this machine (the clocks went back), so it has no single UTC offset - correct it with `kasl pauses` or `kasl report` before sending",
            naive
        ),
        chrono::LocalResult::None => bail!(
            "'{}' never happened on this machine (the clocks went forward), so it has no UTC offset - correct it with `kasl pauses` or `kasl report` before sending",
            naive
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libs::task::Task;
    use chrono::{Duration, NaiveDate};

    /// A workday with fixed bounds, so assertions can name exact instants.
    fn workday(start: &str, end: Option<&str>) -> Workday {
        Workday {
            id: 1,
            date: NaiveDate::parse_from_str(&start[..10], "%Y-%m-%d").unwrap(),
            start: NaiveDateTime::parse_from_str(start, "%Y-%m-%d %H:%M:%S").unwrap(),
            end: end.map(|end| NaiveDateTime::parse_from_str(end, "%Y-%m-%d %H:%M:%S").unwrap()),
        }
    }

    fn stored_task(id: i32, name: &str, completeness: Option<i32>) -> Task {
        let mut task = Task::new(name, "", completeness);
        task.id = Some(id);
        task.timestamp = Some("2026-08-31 10:00:00".to_string());
        task
    }

    #[test]
    fn every_instant_carries_an_offset() {
        // The requirement the server states outright: an instant without an
        // offset is refused there, so none may leave here without one.
        let day = build_from_parts(&workday("2026-08-31 09:00:00", Some("2026-08-31 18:00:00")), &[], &[]).unwrap();

        // The wall-clock reading is untouched - 09:00 stays 09:00 - and the
        // offset makes it name one instant instead of many.
        let wall_clock = NaiveDateTime::parse_from_str("2026-08-31 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(day.started_at.naive_local(), wall_clock);
        assert_eq!(day.started_at, Local.from_local_datetime(&wall_clock).unwrap().fixed_offset());
        assert_eq!(day.date, NaiveDate::from_ymd_opt(2026, 8, 31).unwrap());
    }

    #[test]
    fn an_open_day_has_no_end() {
        // A day still being worked is uploaded without `ended_at` rather than
        // with an invented one: the server reads the absence as "still open".
        let day = build_from_parts(&workday("2026-08-31 09:00:00", None), &[], &[]).unwrap();
        assert!(day.ended_at.is_none());
    }

    #[test]
    fn the_task_set_is_declared_complete() {
        // Without this the server keeps a task the employee deleted here,
        // forever, on their manager's dashboard (ADR 0005).
        let day = build_from_parts(&workday("2026-08-31 09:00:00", None), &[], &[]).unwrap();
        assert!(day.tasks_are_complete);
    }

    #[test]
    fn a_task_carries_both_of_its_ids() {
        let mut task = stored_task(7, "Write the ingest client", Some(50));
        task.task_id = Some(3);

        let day = build_from_parts(&workday("2026-08-31 09:00:00", None), &[], &[task]).unwrap();

        let uploaded = &day.tasks[0];
        assert_eq!(uploaded.agent_task_id, 7, "the row id is what a re-upload matches on");
        assert_eq!(uploaded.agent_group_id, Some(3), "the group id ties the same work across days");
        assert_eq!(uploaded.completeness, 50);
    }

    #[test]
    fn an_empty_comment_is_sent_as_absent() {
        // kasl stores "" for "no comment"; sending it would have the server
        // store and display an empty note as if one had been written.
        let day = build_from_parts(&workday("2026-08-31 09:00:00", None), &[], &[stored_task(1, "Task", Some(0))]).unwrap();
        assert!(day.tasks[0].comment.is_none());
    }

    #[test]
    fn completeness_is_clamped_into_the_range_the_server_accepts() {
        // The server refuses the whole day over one out-of-range task. Losing
        // a day to a local data glitch is the worse outcome of the two.
        let day = build_from_parts(
            &workday("2026-08-31 09:00:00", None),
            &[],
            &[
                stored_task(1, "Over", Some(150)),
                stored_task(2, "Under", Some(-5)),
                stored_task(3, "Unset", None),
            ],
        )
        .unwrap();

        assert_eq!(day.tasks[0].completeness, 100);
        assert_eq!(day.tasks[1].completeness, 0);
        assert_eq!(day.tasks[2].completeness, 0, "an unset completeness means not started");
    }

    #[test]
    fn a_pause_keeps_the_duration_kasl_computed() {
        // Neighbouring pauses are merged before reporting, so the duration is
        // not always `end - start`. Recomputing it on either side would
        // quietly disagree with what `kasl report` showed the employee.
        let pause = Pause {
            id: 1,
            start: NaiveDateTime::parse_from_str("2026-08-31 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
            end: Some(NaiveDateTime::parse_from_str("2026-08-31 12:30:00", "%Y-%m-%d %H:%M:%S").unwrap()),
            duration: Some(Duration::seconds(2400)),
            protected: true,
        };

        let day = build_from_parts(&workday("2026-08-31 09:00:00", None), &[pause], &[]).unwrap();

        assert_eq!(day.pauses[0].duration_seconds, Some(2400));
        assert!(day.pauses[0].manual, "a protected pause is one the employee entered by hand");
    }

    /// A zone that shifts forward one hour at 2026-10-18 00:00 local.
    ///
    /// Hand-built because the machine running these tests may sit in a zone
    /// that never shifts, and the two branches below exist only where one
    /// does. Modelled on the southern-hemisphere shift this project's own
    /// clocks make: -04:00 becomes -03:00, so 00:00-00:59 never happens and,
    /// on the way back, 23:00-23:59 happens twice.
    #[derive(Debug, Clone, Copy)]
    struct ShiftingZone {
        /// Hours east of Greenwich before the shift, and after it.
        before_hours: i32,
        after_hours: i32,
    }

    /// The instant the offset changes, in UTC.
    const SHIFT_AT_UTC: &str = "2026-10-18 04:00:00";

    impl ShiftingZone {
        /// Clocks go forward: an hour of local time is skipped.
        fn forward() -> Self {
            ShiftingZone {
                before_hours: -4,
                after_hours: -3,
            }
        }

        /// Clocks go back: an hour of local time happens twice.
        fn back() -> Self {
            ShiftingZone {
                before_hours: -3,
                after_hours: -4,
            }
        }

        fn before(&self) -> FixedOffset {
            FixedOffset::east_opt(self.before_hours * 3600).unwrap()
        }

        fn after(&self) -> FixedOffset {
            FixedOffset::east_opt(self.after_hours * 3600).unwrap()
        }

        fn shift_instant() -> NaiveDateTime {
            NaiveDateTime::parse_from_str(SHIFT_AT_UTC, "%Y-%m-%d %H:%M:%S").unwrap()
        }
    }

    impl TimeZone for ShiftingZone {
        type Offset = FixedOffset;

        fn from_offset(offset: &FixedOffset) -> Self {
            let hours = offset.local_minus_utc() / 3600;
            ShiftingZone {
                before_hours: hours,
                after_hours: hours,
            }
        }

        fn offset_from_local_date(&self, _local: &NaiveDate) -> chrono::LocalResult<FixedOffset> {
            chrono::LocalResult::Single(self.before())
        }

        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> chrono::LocalResult<FixedOffset> {
            // Each candidate offset is converted back to UTC and checked
            // against the shift: an offset is valid only for the instants it
            // actually covers. `utc = local - offset`.
            let utc_before = *local - Duration::hours(self.before_hours as i64);
            let utc_after = *local - Duration::hours(self.after_hours as i64);

            let valid_before = utc_before < Self::shift_instant();
            let valid_after = utc_after >= Self::shift_instant();

            match (valid_before, valid_after) {
                // Covered by neither: the hour was skipped when the clocks
                // went forward, and never happened here at all.
                (false, false) => chrono::LocalResult::None,
                // Covered by both: the hour repeated when they went back.
                (true, true) => chrono::LocalResult::Ambiguous(self.before(), self.after()),
                (true, false) => chrono::LocalResult::Single(self.before()),
                (false, true) => chrono::LocalResult::Single(self.after()),
            }
        }

        fn offset_from_utc_date(&self, _utc: &NaiveDate) -> FixedOffset {
            self.before()
        }

        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> FixedOffset {
            if *utc >= Self::shift_instant() { self.after() } else { self.before() }
        }
    }

    #[test]
    fn an_hour_that_never_happened_is_refused_rather_than_invented() {
        // With the clocks going forward at 04:00 UTC, local 00:00-00:59 is
        // skipped. Giving 00:30 an offset anyway would file work at a time
        // nobody worked, and nothing downstream would ever question it.
        let skipped = NaiveDateTime::parse_from_str("2026-10-18 00:30:00", "%Y-%m-%d %H:%M:%S").unwrap();

        let error = with_offset_of(&ShiftingZone::forward(), skipped).unwrap_err().to_string();

        assert!(error.contains("never happened"), "unexpected error: {}", error);
        assert!(error.contains("2026-10-18 00:30:00"), "the error should name the instant: {}", error);
    }

    #[test]
    fn an_hour_that_happened_twice_is_refused_rather_than_guessed() {
        // With the clocks going back at the same instant, local 00:00-00:59
        // happens twice. Picking either reading is a coin flip that silently
        // moves an hour of work.
        let repeated = NaiveDateTime::parse_from_str("2026-10-18 00:30:00", "%Y-%m-%d %H:%M:%S").unwrap();

        let error = with_offset_of(&ShiftingZone::back(), repeated).unwrap_err().to_string();

        assert!(error.contains("happened twice"), "unexpected error: {}", error);
        assert!(error.contains("2026-10-18 00:30:00"), "the error should name the instant: {}", error);
    }

    #[test]
    fn the_two_halves_of_a_shifting_day_keep_their_own_offsets() {
        // The reason the offset is taken per timestamp rather than per day:
        // stamping a whole day with one of these would move half its hours.
        let zone = ShiftingZone::forward();
        let before = NaiveDateTime::parse_from_str("2026-10-17 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let after = NaiveDateTime::parse_from_str("2026-10-18 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap();

        assert_eq!(with_offset_of(&zone, before).unwrap().offset(), &zone.before());
        assert_eq!(with_offset_of(&zone, after).unwrap().offset(), &zone.after());
    }

    #[test]
    fn a_task_without_an_id_is_named_rather_than_sent() {
        let mut task = stored_task(1, "Unsaved", Some(0));
        task.id = None;

        let error = build_from_parts(&workday("2026-08-31 09:00:00", None), &[], &[task]).unwrap_err().to_string();

        assert!(error.contains("Unsaved"), "the error should name the task: {}", error);
    }
}
