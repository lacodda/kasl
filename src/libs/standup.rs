//! The day or the week as a markdown summary of what was done: `kasl standup`.
//!
//! ```text
//! $ kasl standup
//! ## Thursday, November 12, 2026
//!
//! Worked 6h 40m, 09:12-17:48.
//!
//! **Done**
//!
//! - [PROJ-412](https://jira.example.com/browse/PROJ-412) Fix session timeout - stale cookie jar
//!
//! **In progress**
//!
//! - PROJ-419 Draft migration for protected pauses (60%) - backfill pending
//! ```
//!
//! Made from what kasl already holds: the hours are each day's [`DayWork`] -
//! the figure `kasl report` shows - and the lines are the day's tasks. A
//! week lists its days' hours and its tasks once each: a task carried from
//! day to day is one chain, and the week says where the chain got to.
//!
//! `--append` puts the summary into a markdown file - a daily note - between
//! two comment markers ([`place`]), so running it again replaces what it
//! wrote rather than adding a second copy.

use crate::db::inbox::Inbox;
use crate::db::tasks::Tasks;
use crate::db::workdays::{Workday, Workdays};
use crate::libs::config::Config;
use crate::libs::formatter::format_span;
use crate::libs::report::DayWork;
use crate::libs::summary::week_bounds;
use crate::libs::task::{Task, TaskFilter};
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime};
use std::collections::HashMap;
use std::path::PathBuf;

/// What a standup sums up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Day(NaiveDate),
    /// The ISO week, Monday to Sunday, the date falls in.
    Week(NaiveDate),
}

impl Period {
    /// The first and the last day it covers.
    pub fn bounds(self) -> (NaiveDate, NaiveDate) {
        match self {
            Period::Day(date) => (date, date),
            Period::Week(date) => week_bounds(date),
        }
    }

    /// What names it in the markers of a note: `2026-11-12`, `2026-W46`.
    pub fn id(self) -> String {
        match self {
            Period::Day(date) => date.format("%Y-%m-%d").to_string(),
            Period::Week(date) => {
                let week = date.iso_week();
                format!("{}-W{:02}", week.year(), week.week())
            }
        }
    }

    /// What names it in a sentence: `Thursday, November 12, 2026`, `week 46 of 2026`.
    pub fn label(self) -> String {
        match self {
            Period::Day(date) => long_date(date),
            Period::Week(date) => {
                let week = date.iso_week();
                format!("week {} of {}", week.week(), week.year())
            }
        }
    }
}

/// One day as the standup reads it.
#[derive(Debug, Clone)]
pub struct Day {
    pub date: NaiveDate,
    /// `None` for a day the watcher never opened.
    pub workday: Option<Workday>,
    /// The day's work, as the report counts it.
    pub work: Duration,
    /// Where the counted work ends: the last interval's end.
    pub last_interval_end: Option<NaiveDateTime>,
    pub tasks: Vec<Task>,
}

impl Day {
    fn is_empty(&self) -> bool {
        self.workday.is_none() && self.tasks.is_empty()
    }
}

/// A day or a week, read once for every way of writing it down.
#[derive(Debug, Clone)]
pub struct Standup {
    pub period: Period,
    /// The days with something recorded on them, in date order.
    pub days: Vec<Day>,
    /// The date it was read on: today's day is "so far".
    pub today: NaiveDate,
    /// Where each issue a task came from lives, by key, when the inbox knows it.
    pub links: HashMap<String, String>,
}

impl Standup {
    /// Reads the period's days, their tasks, and the addresses of their issues.
    pub fn read(period: Period) -> Result<Standup> {
        let monitor = Config::read()?.monitor.unwrap_or_default();
        let (from, to) = period.bounds();
        let mut workdays = Workdays::new()?;
        let mut tasks_db = Tasks::new()?;
        let inbox = Inbox::new()?;

        let mut days = Vec::new();
        let mut links = HashMap::new();
        for date in from.iter_days().take_while(|date| *date <= to) {
            let workday = workdays.fetch(date)?;
            let (work, last_interval_end) = match &workday {
                Some(workday) => {
                    let day = DayWork::of(workday, &monitor)?;
                    (day.work, day.intervals.last().map(|interval| interval.end))
                }
                None => (Duration::zero(), None),
            };
            let tasks = tasks_db.fetch(TaskFilter::Date(date))?;
            for key in tasks.iter().filter_map(|task| task.issue_key.as_ref()) {
                if !links.contains_key(key)
                    && let Some(item) = inbox.get_by_key(key)?
                    && !item.url.is_empty()
                {
                    links.insert(key.clone(), item.url);
                }
            }
            let day = Day {
                date,
                workday,
                work,
                last_interval_end,
                tasks,
            };
            if !day.is_empty() {
                days.push(day);
            }
        }

        Ok(Standup {
            period,
            days,
            today: Local::now().date_naive(),
            links,
        })
    }

    /// Whether nothing was recorded in the period: no workday, no task.
    pub fn is_empty(&self) -> bool {
        self.days.is_empty()
    }

    /// The summary as markdown, without a trailing newline.
    pub fn markdown(&self) -> String {
        match self.period {
            Period::Day(date) => self.day_markdown(date),
            Period::Week(_) => self.week_markdown(),
        }
    }

    fn day_markdown(&self, date: NaiveDate) -> String {
        let mut out = vec![format!("## {}", long_date(date))];
        let Some(day) = self.days.first() else {
            out.push(String::new());
            out.push("Nothing recorded.".to_string());
            return out.join("\n");
        };

        out.push(String::new());
        out.push(self.worked_sentence(day));
        let (done, open): (Vec<&Task>, Vec<&Task>) = day.tasks.iter().partition(|task| is_done(task));
        self.push_tasks(&mut out, &done, &open);
        if day.tasks.is_empty() {
            out.push(String::new());
            out.push("No tasks recorded.".to_string());
        }
        out.join("\n")
    }

    fn week_markdown(&self) -> String {
        let (from, to) = self.period.bounds();
        let mut out = vec![format!("## Week {}, {}", from.iso_week().week(), date_range(from, to))];
        out.push(String::new());

        let worked: Vec<&Day> = self.days.iter().filter(|day| day.workday.is_some()).collect();
        if worked.is_empty() {
            out.push("No working time recorded.".to_string());
        } else {
            let total = worked.iter().fold(Duration::zero(), |sum, day| sum + day.work);
            let count = worked.len();
            out.push(format!(
                "Worked {} over {} {}.",
                format_span(&total),
                count,
                if count == 1 { "day" } else { "days" }
            ));
            out.push(String::new());
            for day in &worked {
                out.push(format!("- {}: {}", day.date.format("%A, %B %-d"), format_span(&day.work)));
            }
        }

        // A task carried across days is one chain; the week says where the
        // chain got to - its last row in the week - and lists it once, in the
        // order the chains first appeared.
        let mut order: Vec<i32> = Vec::new();
        let mut latest: HashMap<i32, &Task> = HashMap::new();
        for task in self.days.iter().flat_map(|day| &day.tasks) {
            let chain = task.task_id.or(task.id).unwrap_or_default();
            if !latest.contains_key(&chain) {
                order.push(chain);
            }
            latest.insert(chain, task);
        }
        let chains: Vec<&Task> = order.iter().filter_map(|chain| latest.get(chain).copied()).collect();
        let (done, open): (Vec<&Task>, Vec<&Task>) = chains.into_iter().partition(|task| is_done(task));
        self.push_tasks(&mut out, &done, &open);
        out.join("\n")
    }

    /// `Worked 6h 40m, 09:12-17:48.`, or `so far, since 09:12` while the day runs.
    fn worked_sentence(&self, day: &Day) -> String {
        let Some(workday) = &day.workday else {
            return "No working time recorded.".to_string();
        };
        let start = workday.start.format("%H:%M");
        let work = format_span(&day.work);
        if workday.end.is_none() && day.date == self.today {
            return format!("Worked {work} so far, since {start}.");
        }
        match workday.end.or(day.last_interval_end) {
            Some(end) => format!("Worked {work}, {start}-{}.", end.format("%H:%M")),
            None => format!("Worked {work}, from {start}."),
        }
    }

    fn push_tasks(&self, out: &mut Vec<String>, done: &[&Task], open: &[&Task]) {
        for (title, tasks) in [("**Done**", done), ("**In progress**", open)] {
            if tasks.is_empty() {
                continue;
            }
            out.push(String::new());
            out.push(title.to_string());
            out.push(String::new());
            out.extend(tasks.iter().map(|task| format!("- {}", self.task_line(task))));
        }
    }

    /// One task: its name with its issue linked, how far it got, its comment.
    fn task_line(&self, task: &Task) -> String {
        let mut line = self.task_name(task);
        if !is_done(task)
            && let Some(completeness) = task.completeness
        {
            line.push_str(&format!(" ({completeness}%)"));
        }
        if !task.comment.trim().is_empty() {
            line.push_str(" - ");
            line.push_str(&escape(task.comment.trim()));
        }
        line
    }

    /// The name, its issue key a link when the inbox knows where the issue lives.
    ///
    /// `kasl inbox take` opens a task's name with the key, so the key is
    /// linked where it stands; a name renamed away from its key gets the
    /// linked key after it.
    fn task_name(&self, task: &Task) -> String {
        let name = task.name.trim();
        let Some((key, url)) = task.issue_key.as_ref().and_then(|key| self.links.get(key).map(|url| (key, url))) else {
            return escape(name);
        };
        let link = format!("[{key}]({url})");
        match name.strip_prefix(key.as_str()) {
            Some(rest) => format!("{link}{}", escape(rest)),
            None => format!("{} ({link})", escape(name)),
        }
    }
}

/// A task with no completeness recorded counts as done, as the task table shows it.
fn is_done(task: &Task) -> bool {
    task.completeness.unwrap_or(100) >= 100
}

/// Escapes what markdown would read as formatting in text kasl did not write.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// `Thursday, November 12, 2026`.
fn long_date(date: NaiveDate) -> String {
    date.format("%A, %B %-d, %Y").to_string()
}

/// `November 9-15, 2026`, `October 26 - November 1, 2026`, `December 29, 2025 - January 4, 2026`.
fn date_range(from: NaiveDate, to: NaiveDate) -> String {
    if from.year() != to.year() {
        format!("{} - {}", from.format("%B %-d, %Y"), to.format("%B %-d, %Y"))
    } else if from.month() != to.month() {
        format!("{} - {}, {}", from.format("%B %-d"), to.format("%B %-d"), to.year())
    } else {
        format!("{}-{}, {}", from.format("%B %-d"), to.day(), to.year())
    }
}

/// The marker a note's standup block opens with.
fn opening(id: &str) -> String {
    format!("<!-- kasl standup {id} -->")
}

/// The marker it closes with; it names the period too, so one note can hold
/// a day's block and a week's without either closing the other.
fn closing(id: &str) -> String {
    format!("<!-- /kasl standup {id} -->")
}

/// Whether [`place`] replaced a block it wrote before, or added a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placed {
    Added,
    Replaced,
}

/// Puts `markdown` into a note's text as the block for `id`.
///
/// The block sits between two HTML comments - markdown renderers, Obsidian
/// among them, show neither - so a second run finds the first one's block and
/// replaces it, and everything around it stays as it was. Without one the
/// block goes at the end, after a blank line. The note's own line endings are
/// kept: a note written with CRLF stays CRLF.
pub fn place(note: &str, id: &str, markdown: &str) -> (String, Placed) {
    let newline = if note.contains("\r\n") { "\r\n" } else { "\n" };
    let block = format!("{}\n{}\n{}", opening(id), markdown, closing(id)).replace('\n', newline);

    let open = opening(id);
    let close = closing(id);
    if let Some(start) = note.find(&open)
        && let Some(length) = note[start..].find(&close)
    {
        let end = start + length + close.len();
        return (format!("{}{}{}", &note[..start], block, &note[end..]), Placed::Replaced);
    }

    let kept = note.trim_end_matches(['\r', '\n']);
    let text = if kept.is_empty() {
        format!("{block}{newline}")
    } else {
        format!("{kept}{newline}{newline}{block}{newline}")
    };
    (text, Placed::Added)
}

/// The file `--append` names, its placeholders filled in for the period.
///
/// `{date}` is the day - a week's Monday - as `YYYY-MM-DD`, the name a daily
/// note usually has; `{week}` is the ISO week, `2026-W46`. A leading `~` is
/// the home directory, for the shells that pass it through unexpanded
/// (PowerShell does, to a program).
pub fn note_path(template: &str, period: Period) -> PathBuf {
    let (from, _) = period.bounds();
    let week = from.iso_week();
    let filled = template
        .replace("{date}", &from.format("%Y-%m-%d").to_string())
        .replace("{week}", &format!("{}-W{:02}", week.year(), week.week()));
    if let Some(rest) = filled.strip_prefix('~')
        && (rest.is_empty() || rest.starts_with(['/', '\\']))
        && let Some(home) = std::env::home_dir()
    {
        return home.join(rest.trim_start_matches(['/', '\\']));
    }
    PathBuf::from(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        date(day).and_hms_opt(hour, minute, 0).unwrap()
    }

    fn task(id: i32, chain: i32, name: &str, completeness: i32, comment: &str) -> Task {
        let mut task = Task::new(name, comment, Some(completeness));
        task.id = Some(id);
        task.task_id = Some(chain);
        task
    }

    fn day(day_of_month: u32, work_minutes: i64, end: Option<(u32, u32)>, tasks: Vec<Task>) -> Day {
        Day {
            date: date(day_of_month),
            workday: Some(Workday {
                id: day_of_month as i32,
                date: date(day_of_month),
                start: at(day_of_month, 9, 12),
                end: end.map(|(hour, minute)| at(day_of_month, hour, minute)),
            }),
            work: Duration::minutes(work_minutes),
            last_interval_end: Some(at(day_of_month, 17, 30)),
            tasks,
        }
    }

    fn standup(period: Period, days: Vec<Day>) -> Standup {
        Standup {
            period,
            days,
            today: date(12),
            links: HashMap::new(),
        }
    }

    #[test]
    fn a_day_splits_its_tasks_into_done_and_in_progress() {
        let tasks = vec![
            task(1, 1, "PROJ-412 Fix session timeout", 100, "stale cookie jar"),
            task(2, 2, "PROJ-419 Draft migration", 60, "backfill pending"),
        ];
        let mut standup = standup(Period::Day(date(11)), vec![day(11, 400, Some((17, 48)), tasks)]);
        standup.days[0].tasks[0].issue_key = Some("PROJ-412".to_string());
        standup
            .links
            .insert("PROJ-412".to_string(), "https://jira.example.com/browse/PROJ-412".to_string());

        assert_eq!(
            standup.markdown(),
            "## Wednesday, November 11, 2026\n\
             \n\
             Worked 6h 40m, 09:12-17:48.\n\
             \n\
             **Done**\n\
             \n\
             - [PROJ-412](https://jira.example.com/browse/PROJ-412) Fix session timeout - stale cookie jar\n\
             \n\
             **In progress**\n\
             \n\
             - PROJ-419 Draft migration (60%) - backfill pending"
        );
    }

    #[test]
    fn today_is_so_far_and_an_unclosed_past_day_ends_at_its_last_interval() {
        let today = standup(Period::Day(date(12)), vec![day(12, 250, None, vec![])]);
        assert!(today.markdown().contains("Worked 4h 10m so far, since 09:12."), "{}", today.markdown());
        assert!(today.markdown().ends_with("No tasks recorded."));

        let unclosed = standup(Period::Day(date(10)), vec![day(10, 480, None, vec![])]);
        assert!(unclosed.markdown().contains("Worked 8h, 09:12-17:30."), "{}", unclosed.markdown());
    }

    #[test]
    fn a_week_lists_each_chain_once_where_it_got_to() {
        // PROJ-419 runs Monday to Wednesday and is done on Wednesday: one line,
        // under Done. The review is still open on Tuesday, its last day.
        let monday = day(9, 480, Some((17, 30)), vec![task(1, 1, "PROJ-419 Draft migration", 30, "")]);
        let tuesday = day(
            10,
            450,
            Some((17, 0)),
            vec![task(2, 1, "PROJ-419 Draft migration", 70, ""), task(3, 3, "Review PR #318", 50, "")],
        );
        let wednesday = day(11, 425, Some((16, 50)), vec![task(4, 1, "PROJ-419 Draft migration", 100, "merged")]);
        let week = standup(Period::Week(date(11)), vec![monday, tuesday, wednesday]);

        assert_eq!(
            week.markdown(),
            "## Week 46, November 9-15, 2026\n\
             \n\
             Worked 22h 35m over 3 days.\n\
             \n\
             - Monday, November 9: 8h\n\
             - Tuesday, November 10: 7h 30m\n\
             - Wednesday, November 11: 7h 5m\n\
             \n\
             **Done**\n\
             \n\
             - PROJ-419 Draft migration - merged\n\
             \n\
             **In progress**\n\
             \n\
             - Review PR #318 (50%)"
        );
    }

    #[test]
    fn a_week_across_months_and_years_names_both_ends() {
        assert_eq!(date_range(date(9), date(15)), "November 9-15, 2026");
        let (from, to) = Period::Week(NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()).bounds();
        assert_eq!(date_range(from, to), "October 26 - November 1, 2026");
        let (from, to) = Period::Week(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()).bounds();
        assert_eq!(date_range(from, to), "December 29, 2025 - January 4, 2026");
        assert_eq!(Period::Week(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()).id(), "2026-W01");
    }

    #[test]
    fn what_kasl_adds_to_the_names_is_ascii() {
        // `kasl standup | clip` hands the bytes to a program that reads the
        // console's code page: kasl's own punctuation must survive it, so only
        // the user's text can carry anything else.
        let week = standup(
            Period::Week(NaiveDate::from_ymd_opt(2026, 10, 28).unwrap()),
            vec![day(9, 480, Some((17, 30)), vec![task(1, 1, "Ship it", 40, "nearly")])],
        );
        assert!(week.markdown().is_ascii(), "{}", week.markdown());
        let day = standup(Period::Day(date(9)), week.days.clone());
        assert!(day.markdown().is_ascii(), "{}", day.markdown());
    }

    #[test]
    fn nothing_recorded_says_so() {
        let empty = standup(Period::Day(date(8)), vec![]);
        assert!(empty.is_empty());
        assert_eq!(empty.markdown(), "## Sunday, November 8, 2026\n\nNothing recorded.");
    }

    #[test]
    fn names_kasl_did_not_write_are_escaped() {
        let standup = standup(Period::Day(date(11)), vec![]);
        let line = standup.task_line(&task(1, 1, "Fix *all* [the] <bugs>", 100, "use `snake_case`"));
        assert_eq!(line, r"Fix \*all\* \[the\] \<bugs\> - use \`snake\_case\`");
    }

    #[test]
    fn a_renamed_task_keeps_its_issue_linked_after_the_name() {
        let mut standup = standup(Period::Day(date(11)), vec![]);
        standup.links.insert("PROJ-1".to_string(), "https://jira.example.com/browse/PROJ-1".to_string());
        let mut renamed = task(1, 1, "Session timeout", 100, "");
        renamed.issue_key = Some("PROJ-1".to_string());
        assert_eq!(
            standup.task_line(&renamed),
            "Session timeout ([PROJ-1](https://jira.example.com/browse/PROJ-1))"
        );
    }

    #[test]
    fn placing_twice_replaces_the_first_block() {
        let note = "# 2026-11-12\n\nMorning notes.\n";
        let (first, placed) = place(note, "2026-11-12", "## Thursday\n\nWorked 1h.");
        assert_eq!(placed, Placed::Added);
        assert_eq!(
            first,
            "# 2026-11-12\n\nMorning notes.\n\n<!-- kasl standup 2026-11-12 -->\n## Thursday\n\nWorked 1h.\n<!-- /kasl standup 2026-11-12 -->\n"
        );

        let edited = format!("{first}\nEvening notes.\n");
        let (second, placed) = place(&edited, "2026-11-12", "## Thursday\n\nWorked 7h.");
        assert_eq!(placed, Placed::Replaced);
        assert_eq!(second.matches("<!-- kasl standup").count(), 1, "{second}");
        assert!(second.contains("Worked 7h.") && !second.contains("Worked 1h."));
        assert!(second.starts_with("# 2026-11-12\n\nMorning notes.\n\n") && second.ends_with("\nEvening notes.\n"));
    }

    #[test]
    fn a_week_block_and_a_day_block_live_side_by_side() {
        let (note, _) = place("", "2026-11-12", "day");
        let (note, placed) = place(&note, "2026-W46", "week");
        assert_eq!(placed, Placed::Added);
        let (note, placed) = place(&note, "2026-11-12", "day again");
        assert_eq!(placed, Placed::Replaced);
        assert!(note.contains("week") && note.contains("day again") && !note.contains("day\n"));
    }

    #[test]
    fn a_crlf_note_stays_crlf() {
        let (note, _) = place("# Notes\r\n", "2026-11-12", "## Day\n\nWorked 1h.");
        assert!(!note.replace("\r\n", "").contains('\n'), "a bare LF slipped in: {note:?}");
    }

    #[test]
    fn the_note_path_fills_in_the_period() {
        assert_eq!(note_path("notes/{date}.md", Period::Day(date(12))), PathBuf::from("notes/2026-11-12.md"));
        assert_eq!(note_path("notes/{week}.md", Period::Week(date(12))), PathBuf::from("notes/2026-W46.md"));
        assert_eq!(note_path("notes/{date}.md", Period::Week(date(12))), PathBuf::from("notes/2026-11-09.md"));
        if let Some(home) = std::env::home_dir() {
            assert_eq!(note_path("~/notes/{date}.md", Period::Day(date(12))), home.join("notes/2026-11-12.md"));
        }
        assert_eq!(note_path("~notes.md", Period::Day(date(12))), PathBuf::from("~notes.md"));
    }
}
