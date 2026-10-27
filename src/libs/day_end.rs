//! Writing a day's end, the one way it is written.
//!
//! Two commands close a day - `kasl end`, and `kasl report --send` before it
//! sends the day - and both come here, so a day's end follows one rule:
//!
//! * **Today ends now.** The person saying they stop is the end, and saying
//!   it again later moves it: they kept working.
//! * **A past day ends at its last observed activity** - the end of its last
//!   pause, whatever its length, as the report already counts an unclosed
//!   past day ([`crate::libs::report::workday_end_time`]). Not at the moment
//!   it is closed: sending yesterday this morning must not stretch yesterday
//!   to this morning.
//! * **A past day already closed keeps its end.** Re-sending it is not a
//!   reason to move it.
//!
//! An end newly written is a `workday_end` for the plugins that hear it.

use crate::db::pauses::Pauses;
use crate::db::workdays::Workdays;
use crate::libs::plugin::events;
use crate::libs::plugin::manifest::Event;
use crate::libs::report::{DayReport, workday_end_time};
use anyhow::Result;
use chrono::{Local, NaiveDate, NaiveDateTime, Timelike};
use tracing::debug;

/// What closing a day came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closed {
    /// The end was written, at this moment.
    Ended(NaiveDateTime),
    /// A past day already had an end, and kept it.
    Kept(NaiveDateTime),
    /// No workday was recorded on the date; nothing was written.
    NeverStarted,
}

/// Writes the end of `date`'s workday by the rule above.
///
/// Says nothing to plugins: a command that closes a day says so first, then
/// calls [`tell`] when an end was written, so the plugins' lines follow its own.
pub fn close(date: NaiveDate) -> Result<Closed> {
    let mut workdays = Workdays::new()?;
    let Some(workday) = workdays.fetch(date)? else {
        return Ok(Closed::NeverStarted);
    };
    let now = Local::now().naive_local().with_nanosecond(0).unwrap_or_else(|| Local::now().naive_local());
    let end = if date == now.date() {
        // A start stamped a moment ahead of the clock - a corrected system
        // time - must not give the day an end before its start.
        now.max(workday.start)
    } else if let Some(end) = workday.end {
        return Ok(Closed::Kept(end));
    } else {
        workday_end_time(&workday, &Pauses::new()?.get_workday_pauses(&workday)?)
    };
    workdays.update_end(date, Some(end))?;
    Ok(Closed::Ended(end))
}

/// Tells the plugins that hear `workday_end` that `date` ended.
///
/// The day they are handed is read now, after the end is in place, so it is
/// the day as `kasl report --json` prints it, its end included.
pub fn tell(date: NaiveDate) {
    match day_document(date) {
        Ok(subject) => events::tell(Event::WorkdayEnd, &subject),
        Err(error) => debug!("plugins: could not read {date} for workday_end: {error:#}"),
    }
}

/// The day as `kasl report --json` prints it: what every event about a day hands over.
pub fn day_document(date: NaiveDate) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(DayReport::read(date)?.to_json())?)
}
