//! Where today stands, in one line for a shell prompt or a status bar: `kasl status`.
//!
//! ```text
//! $ kasl status
//! 6h 40m | paused 12m | inbox 9 (3 new)
//! ```
//!
//! The line is made of segments, each there only when it has something to
//! say, and nothing to say prints nothing - so a prompt segment built on it
//! disappears instead of showing an empty bracket:
//!
//! - **the day** - today's work so far, the figure `kasl report` shows;
//! - **the watcher** - `paused 12m` while it sees a pause, `watch off` when no
//!   watcher runs, because then nothing is being recorded at all;
//! - **the inbox** - what is waiting, with the new ones in parentheses.
//!
//! Every figure comes from the function its own command uses - the day from
//! [`DayWork`], the inbox from [`Inbox::counts`], the watcher's state from
//! [`pulse::presence`] - so the prompt can never disagree with `kasl report`,
//! `kasl inbox` or what the team server is told.
//!
//! It reads local files only: no network, no update check. A prompt runs it
//! every time it draws.

use crate::api::kasl_server::AgentState;
use crate::db::inbox::{Inbox, InboxCounts};
use crate::db::pauses::Pauses;
use crate::db::workdays::{Workday, Workdays};
use crate::libs::config::Config;
use crate::libs::formatter::format_span;
use crate::libs::report::DayWork;
use crate::libs::{daemon, json, pulse};
use anyhow::Result;
use chrono::{Duration, Local, NaiveDate, NaiveDateTime};

/// Today as `kasl status` reads it, once for the line and for `--json`.
#[derive(Debug, Clone)]
pub struct Status {
    /// The moment it was read; the pause's length is measured to it.
    pub now: NaiveDateTime,
    /// `None` until the watcher has opened today's workday.
    pub workday: Option<Workday>,
    /// Today's work so far, as the report counts it.
    pub work: Duration,
    /// When the running watcher started; `None` when no watcher runs.
    pub watcher_since: Option<NaiveDateTime>,
    /// When the pause the running watcher has open began.
    pub paused_since: Option<NaiveDateTime>,
    pub inbox: InboxCounts,
}

impl Status {
    /// Reads today: the workday and its work, the watcher and its pause, the inbox.
    pub fn read() -> Result<Status> {
        let now = Local::now().naive_local();
        let workday = Workdays::new()?.fetch(now.date())?;
        let work = match &workday {
            Some(workday) => DayWork::of(workday, &Config::read()?.monitor.unwrap_or_default())?.work,
            None => Duration::zero(),
        };
        let watcher_since = daemon::running_since();
        let paused_since = match watcher_since {
            Some(since) => Pauses::new()?.open_since(since)?,
            None => None,
        };
        Ok(Status {
            now,
            workday,
            work,
            watcher_since,
            paused_since,
            inbox: Inbox::new()?.counts()?,
        })
    }

    pub fn date(&self) -> NaiveDate {
        self.now.date()
    }

    /// What the watcher sees, in the pulse's words; `None` when no watcher runs.
    ///
    /// A pause only counts inside an open workday, as it does for the pulse:
    /// a day closed with `kasl end` is over even while the keyboard is quiet.
    pub fn state(&self) -> Option<AgentState> {
        self.watcher_since?;
        Some(pulse::presence(self.paused_since.is_some(), self.workday.as_ref()))
    }

    /// When the pause began, while the watcher is in one.
    pub fn paused_at(&self) -> Option<NaiveDateTime> {
        (self.state() == Some(AgentState::Paused)).then_some(self.paused_since).flatten()
    }

    /// The line `kasl status` prints; empty when there is nothing to say.
    pub fn line(&self) -> String {
        let mut segments = Vec::new();
        if self.workday.is_some() {
            segments.push(format_span(&self.work));
        }
        match (self.state(), self.paused_at()) {
            (None, _) => segments.push("watch off".to_string()),
            (Some(AgentState::Paused), Some(since)) => segments.push(format!("paused {}", format_span(&(self.now - since)))),
            _ => {}
        }
        if self.inbox.total > 0 {
            segments.push(if self.inbox.fresh > 0 {
                format!("inbox {} ({} new)", self.inbox.total, self.inbox.fresh)
            } else {
                format!("inbox {}", self.inbox.total)
            });
        }
        segments.join(" | ")
    }

    /// Today as `kasl status --json` prints it.
    pub fn to_json(&self) -> json::Status {
        json::Status {
            date: self.date(),
            workday: self.workday.as_ref().map(|workday| json::Workday {
                start: workday.start,
                end: workday.end,
            }),
            work_seconds: json::seconds(&self.work),
            watcher: self.watcher_since.is_some(),
            state: self.state().map(|state| state.as_str()),
            paused_since: self.paused_at(),
            inbox: self.inbox.into(),
            line: self.line(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 12).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn open_day() -> Workday {
        Workday {
            id: 1,
            date: at(9, 0).date(),
            start: at(9, 0),
            end: None,
        }
    }

    fn status() -> Status {
        Status {
            now: at(15, 52),
            workday: Some(open_day()),
            work: Duration::minutes(6 * 60 + 40),
            watcher_since: Some(at(8, 55)),
            paused_since: None,
            inbox: InboxCounts::default(),
        }
    }

    #[test]
    fn a_working_day_is_its_hours() {
        assert_eq!(status().line(), "6h 40m");
    }

    #[test]
    fn a_pause_says_how_long_it_has_lasted() {
        let paused = Status {
            paused_since: Some(at(15, 40)),
            ..status()
        };
        assert_eq!(paused.line(), "6h 40m | paused 12m");
        assert_eq!(paused.to_json().state, Some("paused"));
        assert_eq!(paused.to_json().paused_since, Some(at(15, 40)));
    }

    #[test]
    fn a_pause_after_the_day_ended_is_not_a_break() {
        // `kasl end` closed the day; the watcher still opens pauses on a quiet
        // keyboard, and the pulse calls that idle - so does the line.
        let ended = Status {
            workday: Some(Workday {
                end: Some(at(15, 0)),
                ..open_day()
            }),
            paused_since: Some(at(15, 40)),
            ..status()
        };
        assert_eq!(ended.line(), "6h 40m");
        assert_eq!(ended.to_json().state, Some("idle"));
        assert_eq!(ended.to_json().paused_since, None);
    }

    #[test]
    fn no_watcher_is_said_whatever_else_there_is() {
        let off = Status {
            watcher_since: None,
            paused_since: None,
            ..status()
        };
        assert_eq!(off.line(), "6h 40m | watch off");
        assert_eq!(off.to_json().state, None);
        assert!(!off.to_json().watcher);

        let off_before_the_day = Status { workday: None, ..off };
        assert_eq!(off_before_the_day.line(), "watch off");
    }

    #[test]
    fn the_inbox_counts_what_waits_and_names_the_new() {
        let inbox = |total, fresh| Status {
            inbox: InboxCounts { total, taken: 0, fresh },
            ..status()
        };
        assert_eq!(inbox(9, 3).line(), "6h 40m | inbox 9 (3 new)");
        assert_eq!(inbox(9, 0).line(), "6h 40m | inbox 9");
        assert_eq!(inbox(0, 0).line(), "6h 40m");
    }

    #[test]
    fn the_line_is_ascii_so_no_code_page_can_garble_it() {
        // PowerShell decodes what a prompt captures in the console's code
        // page; a `·` came back as `┬╖` under 866. Nothing in the line is
        // the user's text, so it can stay ASCII under every code page.
        let busy = Status {
            paused_since: Some(at(15, 40)),
            inbox: InboxCounts { total: 9, taken: 2, fresh: 3 },
            ..status()
        };
        assert!(busy.line().is_ascii(), "{:?}", busy.line());
        let off = Status { watcher_since: None, ..busy };
        assert!(off.line().is_ascii(), "{:?}", off.line());
    }

    #[test]
    fn nothing_to_say_is_an_empty_line() {
        // Watcher running, no day yet, empty inbox: the prompt segment must
        // vanish, not show an empty pair of brackets.
        let quiet = Status { workday: None, ..status() };
        assert_eq!(quiet.line(), "");
        assert_eq!(quiet.to_json().line, "");
        assert_eq!(quiet.to_json().state, Some("idle"));
    }
}
