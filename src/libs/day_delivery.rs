//! Getting days to kasl-server, including the ones that did not go the first
//! time.
//!
//! `kasl server push` sends today and is done. This is what happens when it
//! cannot: the date is written to an outbox, and the next successful run pays
//! the whole debt off in one request.
//!
//! Three rules shape it, and each exists because of a way the naive version
//! goes wrong:
//!
//! * **A failure is sorted before it is stored.** A day the server will never
//!   accept as sent (`4xx`) is not queued - queuing it creates a row that
//!   retries forever and never succeeds. Only a day the server could not
//!   answer for (`5xx`, `429`, no connection at all) is worth keeping.
//! * **A queued day is rebuilt, not replayed.** The outbox holds a date; the
//!   payload is assembled at delivery. A day corrected while the network was
//!   down arrives corrected.
//! * **A backlog is one request.** The batch endpoint answers per day, so one
//!   day the server refuses does not strand the rest behind it (ADR 0005 in
//!   kasl-server).

use crate::api::kasl_server::{DayResult, DayUpload, KaslServer, UploadError};
use crate::db::server_outbox::ServerOutbox;
use crate::libs::day_upload::build_day_upload;
use anyhow::Result;
use chrono::NaiveDate;

/// How many days go in one request.
///
/// The server caps a batch and answers `413` past it (`KASL_MAX_BATCH_DAYS`,
/// ADR 0005), so a long backlog is split here rather than bounced there. Well
/// under any plausible server limit: the cost of an extra request is one round
/// trip, the cost of guessing too high is a refusal the user has to decode.
pub const BATCH_SIZE: usize = 30;

/// What became of one date in a delivery run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivered {
    /// The server stored it. Any queue entry for the date is gone.
    Accepted {
        date: NaiveDate,
        pauses: usize,
        tasks: usize,
        deleted_tasks: u64,
    },

    /// The server refused it, and would refuse it again. Dropped from the
    /// queue rather than retried forever; the reason is carried so it can be
    /// said out loud.
    Refused { date: NaiveDate, reason: String },

    /// The day could not be built or sent this time, and is still owed.
    Deferred { date: NaiveDate, reason: String },
}

impl Delivered {
    /// The date this outcome is about.
    pub fn date(&self) -> NaiveDate {
        match self {
            Delivered::Accepted { date, .. } | Delivered::Refused { date, .. } | Delivered::Deferred { date, .. } => *date,
        }
    }
}

/// Sends `dates` to the server, updating the outbox for each.
///
/// A date whose day no longer exists locally is dropped from the queue rather
/// than carried: the workday was deleted after it was queued, and there is
/// nothing left to owe. Silently retrying it forever would be the alternative.
///
/// The return is one outcome per date that had something to send, in the
/// order the dates were given.
pub async fn deliver(client: &KaslServer, token: &str, outbox: &mut ServerOutbox, dates: &[NaiveDate]) -> Result<Vec<Delivered>> {
    let mut outcomes = Vec::new();
    let mut sendable: Vec<(NaiveDate, DayUpload)> = Vec::new();

    for &date in dates {
        match build_day_upload(date) {
            // Nothing to send: the workday is gone from the database, so the
            // debt is gone with it.
            Ok(None) => {
                outbox.remove(date)?;
            }
            Ok(Some(day)) => sendable.push((date, day)),
            // A day that cannot be assembled is a local problem - a timestamp
            // with no valid offset, a task without an id - and no amount of
            // retrying fixes it from here. It stays queued, because the fix
            // is an edit the user makes and then the day goes.
            Err(error) => {
                let reason = error.to_string();
                outbox.enqueue(date, &reason)?;
                outcomes.push(Delivered::Deferred { date, reason });
            }
        }
    }

    for chunk in sendable.chunks(BATCH_SIZE) {
        let days: Vec<DayUpload> = chunk.iter().map(|(_, day)| day.clone()).collect();
        let dates: Vec<NaiveDate> = chunk.iter().map(|(date, _)| *date).collect();

        match client.upload_batch(token, &days).await {
            Ok(result) => {
                // Read per day, never by the status: a batch answers 200 with
                // refused days inside it (ADR 0005).
                let mut answered: Vec<NaiveDate> = Vec::with_capacity(result.results.len());
                for day_result in &result.results {
                    let outcome = record(outbox, day_result)?;
                    answered.push(outcome.date());
                    outcomes.push(outcome);
                }

                // A day sent but not reported on stays owed. Which day went
                // unanswered is not inferable from position - the server
                // names each date it answers for - so the dates are compared
                // rather than the counts. Keeping a day the server did store
                // costs one harmless re-upload; dropping one it did not
                // loses the day for good.
                for date in dates.iter().filter(|date| !answered.contains(date)) {
                    let reason = "the server did not report on this day".to_string();
                    outbox.enqueue(*date, &reason)?;
                    outcomes.push(Delivered::Deferred { date: *date, reason });
                }
            }
            // The request itself failed. Whether these days are worth keeping
            // is the same question the single-day path asks.
            Err(error) => {
                let retryable = error.is_retryable();
                let reason = error.to_string();
                for date in dates {
                    outcomes.push(settle(outbox, date, &reason, retryable)?);
                }
            }
        }
    }

    Ok(outcomes)
}

/// Applies one day's reported fate to the outbox.
fn record(outbox: &mut ServerOutbox, result: &DayResult) -> Result<Delivered> {
    match result {
        DayResult::Accepted { day } => {
            outbox.remove(day.date)?;
            Ok(Delivered::Accepted {
                date: day.date,
                pauses: day.pauses,
                tasks: day.tasks,
                deleted_tasks: day.deleted_tasks,
            })
        }
        // A day refused inside a batch was refused on its own merits - the
        // server validated it and said no. Sending it again unchanged would
        // get the same answer, so it leaves the queue.
        DayResult::Rejected { date, error } => {
            outbox.remove(*date)?;
            Ok(Delivered::Refused {
                date: *date,
                reason: error.clone(),
            })
        }
    }
}

/// Queues or drops one date after a failure, by whether a retry could work.
fn settle(outbox: &mut ServerOutbox, date: NaiveDate, reason: &str, retryable: bool) -> Result<Delivered> {
    if retryable {
        outbox.enqueue(date, reason)?;
        Ok(Delivered::Deferred {
            date,
            reason: reason.to_string(),
        })
    } else {
        // Never going to be accepted as sent. Keeping it would build a queue
        // of days that retry forever and never leave.
        outbox.remove(date)?;
        Ok(Delivered::Refused {
            date,
            reason: reason.to_string(),
        })
    }
}

/// Records the outcome of a single-day upload, so `push` feeds the same queue.
///
/// Split from [`deliver`] because the single-day path has already sent its
/// day and only needs the bookkeeping; sharing the decision is the point,
/// since the two paths disagreeing about what is worth retrying is exactly
/// the bug this shape prevents.
pub fn record_single(outbox: &mut ServerOutbox, date: NaiveDate, error: &UploadError) -> Result<Delivered> {
    settle(outbox, date, &error.to_string(), error.is_retryable())
}

// The batch size is exercised where it can actually be observed - by counting
// the requests a backlog longer than it produces (`tests/day_delivery.rs`).
// Asserting the constant against a literal here would only restate the line
// above it.
