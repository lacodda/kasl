//! The demo: eight invented weeks, kept apart from the person's own records.
//!
//! `kasl demo` builds them, and `kasl demo <command>` runs any command
//! against them - `kasl demo report`, `kasl demo sum`, `kasl demo inbox`. A
//! newcomer sees every report filled in within the first minute, and the
//! documentation's transcripts are taken from a real build with nothing to
//! hide in them.
//!
//! The demo is a data directory of its own, `demo` inside kasl's: its own
//! database, its own config, its own salt. Entering it ([`enter`]) moves
//! every path kasl resolves, so no command needs to know it runs in a demo
//! and none can reach the real records from inside one. What does reach
//! outside a data directory is closed separately: the OS keyring answers
//! under another service name, plugins of the machine are not looked for,
//! and the commands that act on the machine itself - the watcher, autostart,
//! the team server - are refused (`Cli::dispatch`).
//!
//! The weeks are generated, not stored (ADR 0016): weekdays only, ending
//! today, with today's day open - so the default view of every command has
//! something in it however old the release. A day's hours depend on its
//! date alone, and the generator carries its own xorshift rather than a
//! crate whose stream may change with a version: two demos built on one day
//! are the same demo, and a screenshot can be taken again.

use crate::db::db::Db;
use crate::libs::data_storage::DataStorage;
use anyhow::{Context, Result, bail};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

/// The demo's directory inside kasl's data directory.
pub const DIR: &str = "demo";

/// How many weeks the demo holds, the current one included.
pub const WEEKS: i64 = 8;

/// The file that says when, and by which version, the demo was built.
const MARKER_FILE: &str = "demo.json";

/// The project every invented issue belongs to.
const PROJECT: &str = "ORBIT";

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Makes this run a demo run: from here on every path is the demo's.
pub fn enter() {
    ACTIVE.store(true, Ordering::Relaxed);
}

/// Whether this run works on the demo.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Whether `args` (the program name first) ask for a command to be run on the demo.
///
/// `kasl demo report` does; `kasl demo` and `kasl demo --reset` are the demo
/// command itself. Flags before the command name (`--plain`) are stepped over.
/// Returns the position of the word `demo`, for the caller to take out.
pub fn prefix(args: &[String]) -> Option<usize> {
    let at = args.iter().skip(1).position(|arg| !arg.starts_with('-'))? + 1;
    let next = args.get(at + 1)?;
    (args[at] == "demo" && !next.starts_with('-')).then_some(at)
}

/// When and by what the demo on disk was built.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Marker {
    built: NaiveDate,
    version: String,
}

/// What [`ensure`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ensured {
    /// Built today by this version: left as it is, with whatever was added to it.
    Kept,
    /// There was none.
    Built,
    /// Built on another day or by another version: made anew, so it ends today.
    Rebuilt,
}

/// Makes sure the demo exists and ends today.
///
/// A demo built yesterday has no today in it, and one built by another
/// version may hold what this one's generator no longer writes; either is
/// made anew rather than patched. What a person added while trying kasl goes
/// with it: the demo is a place to try things, not to keep them.
pub fn ensure(now: NaiveDateTime) -> Result<Ensured> {
    let found = marker();
    let wanted = Marker {
        built: now.date(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    if found.as_ref() == Some(&wanted) {
        return Ok(Ensured::Kept);
    }
    build(now)?;
    Ok(if found.is_some() { Ensured::Rebuilt } else { Ensured::Built })
}

fn marker() -> Option<Marker> {
    let path = DataStorage::new().base().join(MARKER_FILE);
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Builds the demo anew, replacing the one on disk.
pub fn build(now: NaiveDateTime) -> Result<Summary> {
    if !active() {
        bail!("the demo is built from inside a demo run");
    }
    let dir = DataStorage::new().base().to_path_buf();
    // The one place kasl deletes a directory: checked by name as well as by
    // mode, so no path but the demo's own can ever be the one removed.
    if dir.file_name().and_then(|name| name.to_str()) != Some(DIR) {
        bail!("{} is not the demo's directory", dir.display());
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("cannot clear the old demo in {}", dir.display()))?;
    }

    let demo = Demo::generate(now);
    let mut db = Db::new()?;
    demo.write(&mut db.conn)?;
    drop(db);

    let marker = Marker {
        built: now.date(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    std::fs::write(DataStorage::new().get_path(MARKER_FILE)?, serde_json::to_string_pretty(&marker)?)?;
    Ok(demo.summary())
}

/// What a built demo holds, for the line `kasl demo` says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: usize,
    pub tasks: usize,
    pub issues: usize,
}

/// The invented records, before they are written.
#[derive(Debug, Clone, PartialEq)]
pub struct Demo {
    pub days: Vec<Day>,
    pub tags: Vec<(&'static str, &'static str)>,
    pub templates: Vec<Template>,
    pub tasks: Vec<TaskRow>,
    pub issues: Vec<Issue>,
    from: NaiveDate,
    to: NaiveDate,
}

/// One workday with its pauses.
#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    pub date: NaiveDate,
    pub start: NaiveDateTime,
    /// `None` for today's day, which is still running.
    pub end: Option<NaiveDateTime>,
    pub pauses: Vec<Pause>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pause {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    /// Recorded by hand, with a reason.
    pub reason: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub name: &'static str,
    pub task_name: &'static str,
    pub comment: &'static str,
    pub completeness: i32,
}

/// One daily row of a task; rows of one task share `chain`.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskRow {
    /// Which task this is a day of: an index among the demo's tasks.
    pub chain: usize,
    pub at: NaiveDateTime,
    pub name: String,
    pub comment: &'static str,
    pub completeness: i32,
    pub issue_key: Option<String>,
    pub tags: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub key: String,
    pub summary: &'static str,
    pub status: (&'static str, &'static str),
    pub priority: (&'static str, i32),
    pub score: f64,
    pub issue_type: &'static str,
    pub first_seen: NaiveDateTime,
    pub pinned: bool,
    pub dismissed: bool,
    pub taken_at: Option<NaiveDateTime>,
    pub snoozed_until: Option<NaiveDateTime>,
    pub gone_at: Option<NaiveDateTime>,
    pub change: Option<(&'static str, NaiveDateTime)>,
}

/// A piece of work the invented person does: a name, how many days it takes.
struct Work {
    /// The number of its issue in [`PROJECT`]; `None` for work no tracker asked for.
    issue: Option<u32>,
    name: &'static str,
    comment: &'static str,
    tags: &'static [&'static str],
    days: u32,
}

const fn work(issue: u32, name: &'static str, comment: &'static str, tags: &'static [&'static str], days: u32) -> Work {
    Work {
        issue: Some(issue),
        name,
        comment,
        tags,
        days,
    }
}

const fn own(name: &'static str, comment: &'static str, tags: &'static [&'static str], days: u32) -> Work {
    Work {
        issue: None,
        name,
        comment,
        tags,
        days,
    }
}

/// The backlog, in the order it is worked through - two pieces at a time.
/// Long enough for eight weeks; it wraps around if a longer demo is ever asked for.
const BACKLOG: &[Work] = &[
    work(398, "Retry uploads that time out", "back off, give up after five tries", &["backend"], 3),
    work(401, "Pass list loses its filter on reload", "", &["frontend", "bug"], 2),
    own("Write the on-call handbook page", "who to wake, and when not to", &["docs"], 2),
    work(
        404,
        "Ground track drifts after midnight UTC",
        "a day boundary, not the maths",
        &["backend", "bug"],
        4,
    ),
    work(407, "Show the next pass on the station card", "", &["frontend"], 3),
    work(409, "Export passes as CSV", "the planners asked twice", &["backend"], 2),
    own("Tidy the CI cache", "builds were restoring a stale one", &["infra"], 1),
    work(
        412,
        "Elevation mask per antenna",
        "settings page and the predictor",
        &["backend", "frontend"],
        5,
    ),
    work(415, "Timeline scrolls back on every refresh", "", &["frontend", "bug"], 2),
    work(418, "Alert when telemetry goes quiet", "ten minutes, then page", &["backend"], 4),
    own("Review the predictor benchmarks", "numbers for the quarterly note", &["review"], 2),
    work(421, "Dark map tiles", "", &["frontend"], 3),
    work(424, "Reject schedules that overlap", "validate on save, say which two", &["backend", "bug"], 3),
    work(427, "Station health on one screen", "what the night shift looks at", &["frontend"], 5),
    own("Upgrade the map library", "two breaking changes in the layers API", &["frontend", "infra"], 3),
    work(431, "Keep a week of raw telemetry", "then thin it to minutes", &["backend", "infra"], 4),
    work(434, "Pass report as PDF", "", &["backend", "docs"], 3),
    work(437, "Time zone picker forgets the choice", "", &["frontend", "bug"], 1),
    work(440, "Doppler correction in the pass table", "", &["backend"], 4),
    own("Write the release notes", "", &["docs"], 1),
    work(
        443,
        "Antenna hand-over between stations",
        "the last minute of one pass, the first of the next",
        &["backend"],
        5,
    ),
    work(446, "Keyboard shortcuts on the timeline", "", &["frontend"], 2),
    work(449, "Orbit refresh fails without a network", "keep the last good set", &["backend", "bug"], 3),
    work(452, "Group stations by region", "", &["frontend"], 3),
    own("Profile the predictor", "half the time is one allocation", &["backend", "review"], 2),
    work(455, "Signal strength history", "", &["backend", "frontend"], 4),
    work(458, "Print view for the weekly plan", "", &["frontend", "docs"], 2),
    work(461, "Predictor disagrees near the poles", "", &["backend", "bug"], 4),
    work(464, "Remember the last station opened", "", &["frontend"], 1),
    work(467, "Audit log for schedule changes", "who moved what, and when", &["backend"], 4),
    work(470, "Legend for the coverage map", "", &["frontend", "docs"], 2),
    work(473, "Pass search by satellite", "", &["backend", "frontend"], 3),
];

/// The tags, with their colours.
const TAGS: &[(&str, &str)] = &[
    ("backend", "blue"),
    ("frontend", "green"),
    ("bug", "red"),
    ("docs", "yellow"),
    ("infra", "magenta"),
    ("review", "cyan"),
    ("meeting", "white"),
];

const TEMPLATES: &[Template] = &[
    Template {
        name: "review",
        task_name: "Code review",
        comment: "",
        completeness: 100,
    },
    Template {
        name: "planning",
        task_name: "Sprint planning",
        comment: "",
        completeness: 100,
    },
    Template {
        name: "retro",
        task_name: "Team retrospective",
        comment: "",
        completeness: 100,
    },
];

/// What waits in the inbox beside the work in hand: number, summary, type, priority, score.
const WAITING: &[(u32, &str, &str, (&str, i32), f64)] = &[
    (476, "Pass times are an hour off after the clock change", "Bug", ("Highest", 1), 9.0),
    (479, "Uplink window shown for a station under maintenance", "Bug", ("High", 2), 8.0),
    (482, "Compare two predictions side by side", "Story", ("Medium", 3), 6.5),
    (485, "Notify the planner when a pass is cancelled", "Story", ("High", 2), 6.0),
    (488, "Map legend overlaps the zoom buttons on a phone", "Bug", ("Medium", 3), 5.0),
    (491, "Archive stations that were decommissioned", "Task", ("Low", 4), 3.5),
    (494, "Colour passes by elevation", "Story", ("Low", 4), 3.0),
    (497, "Spell out UTC in every tooltip", "Task", ("Low", 4), 2.0),
    (499, "Import the legacy schedule format", "Story", ("Medium", 3), 4.0),
];

const STATUS_TODO: (&str, &str) = ("10000", "To Do");
const STATUS_IN_PROGRESS: (&str, &str) = ("10001", "In Progress");
const STATUS_IN_REVIEW: (&str, &str) = ("10002", "In Review");
const STATUS_DONE: (&str, &str) = ("10003", "Done");

/// A small deterministic generator: xorshift64*, seeded per day.
struct Rng(u64);

impl Rng {
    fn for_date(date: NaiveDate, stream: u64) -> Self {
        // Never zero: xorshift stays at zero once there.
        let seed = ((date.num_days_from_ce() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ stream.wrapping_mul(0xbf58_476d_1ce4_e5b9)) | 1;
        let mut rng = Rng(seed);
        // The first outputs of nearby seeds are alike; step past them.
        for _ in 0..4 {
            rng.next();
        }
        rng
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A whole number in `from..to`.
    fn between(&mut self, from: i64, to: i64) -> i64 {
        from + (self.next() % (to - from) as u64) as i64
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// What kind of day a date is, beyond an ordinary one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Ordinary,
    /// Not worked: a day of leave.
    Off,
    /// Ten hours and more.
    Long,
    /// An appointment in the middle of it, recorded by hand.
    Appointment,
    /// Interrupted all day: the one day under the productivity minimum.
    Scattered,
}

fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

/// The Monday the demo starts on: seven weeks before this week's.
fn first_day(today: NaiveDate) -> NaiveDate {
    let monday = today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
    monday - Duration::weeks(WEEKS - 1)
}

/// The shape of a past day. The marked days sit at fixed places in the
/// window, so every demo has one of each; days of leave fall by the date.
fn shape(date: NaiveDate, first: NaiveDate) -> Shape {
    let week = (date - first).num_days() / 7;
    match (week, date.weekday()) {
        (2, Weekday::Thu) | (5, Weekday::Fri) => Shape::Off,
        (3, Weekday::Tue) => Shape::Appointment,
        (4, Weekday::Wed) | (6, Weekday::Mon) => Shape::Long,
        (5, Weekday::Wed) => Shape::Scattered,
        _ => Shape::Ordinary,
    }
}

impl Demo {
    /// The demo as it reads at `now`: every weekday of the last eight weeks, today's still open.
    pub fn generate(now: NaiveDateTime) -> Demo {
        let today = now.date();
        let first = first_day(today);

        let mut days = Vec::new();
        for date in first.iter_days().take_while(|date| *date <= today) {
            if !is_weekday(date) {
                continue;
            }
            let day = if date == today {
                Day::running(date, now)
            } else {
                match shape(date, first) {
                    Shape::Off => None,
                    shape => Some(Day::past(date, shape)),
                }
            };
            days.extend(day);
        }

        let (tasks, in_hand, finished) = schedule(&days, now);
        let issues = inbox(now, &in_hand, &finished);
        Demo {
            days,
            tags: TAGS.to_vec(),
            templates: TEMPLATES.to_vec(),
            tasks,
            issues,
            from: first,
            to: today,
        }
    }

    pub fn summary(&self) -> Summary {
        Summary {
            from: self.from,
            to: self.to,
            days: self.days.len(),
            tasks: self.tasks.iter().map(|row| row.chain).collect::<std::collections::BTreeSet<_>>().len(),
            issues: self.issues.len(),
        }
    }

    /// Writes the records in one transaction, in the spellings the commands write them in.
    ///
    /// Not through the commands' own writers: those stamp the present moment,
    /// and a history has to be written with its dates. What keeps this from
    /// drifting from them is the test that runs every reading command on a
    /// built demo.
    fn write(&self, conn: &mut rusqlite::Connection) -> Result<()> {
        const MOMENT: &str = "%Y-%m-%d %H:%M:%S";
        let moment = |at: &NaiveDateTime| at.format(MOMENT).to_string();
        let tx = conn.transaction()?;

        for day in &self.days {
            tx.execute(
                "INSERT INTO workdays (date, start, end) VALUES (?1, ?2, ?3)",
                params![day.date.format("%Y-%m-%d").to_string(), moment(&day.start), day.end.as_ref().map(moment)],
            )?;
            for pause in &day.pauses {
                tx.execute(
                    "INSERT INTO pauses (start, end, duration, protected, reason) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        moment(&pause.start),
                        moment(&pause.end),
                        (pause.end - pause.start).num_seconds(),
                        pause.reason.is_some(),
                        pause.reason
                    ],
                )?;
            }
        }

        for (name, color) in &self.tags {
            tx.execute("INSERT INTO tags (name, color) VALUES (?1, ?2)", params![name, color])?;
        }
        for template in &self.templates {
            tx.execute(
                "INSERT INTO task_templates (name, task_name, comment, completeness) VALUES (?1, ?2, ?3, ?4)",
                params![template.name, template.task_name, template.comment, template.completeness],
            )?;
        }

        // A task is a chain of daily rows; the chain is named by its first row's id.
        let mut chains: std::collections::HashMap<usize, i64> = std::collections::HashMap::new();
        for row in &self.tasks {
            tx.execute(
                "INSERT INTO tasks (task_id, timestamp, name, comment, completeness, excluded_from_search, issue_key)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
                params![chains.get(&row.chain), moment(&row.at), row.name, row.comment, row.completeness, row.issue_key],
            )?;
            let id = tx.last_insert_rowid();
            if !chains.contains_key(&row.chain) {
                chains.insert(row.chain, id);
                tx.execute("UPDATE tasks SET task_id = ?1 WHERE id = ?1", params![id])?;
                for tag in row.tags {
                    tx.execute(
                        "INSERT OR IGNORE INTO task_tags (task_id, tag_id) SELECT ?1, id FROM tags WHERE name = ?2",
                        params![id, tag],
                    )?;
                }
            }
        }

        for (id, name) in [STATUS_TODO, STATUS_IN_PROGRESS, STATUS_IN_REVIEW, STATUS_DONE] {
            tx.execute("INSERT INTO inbox_statuses (id, name) VALUES (?1, ?2)", params![id, name])?;
        }
        for (name, rank) in [("Highest", 1), ("High", 2), ("Medium", 3), ("Low", 4)] {
            tx.execute("INSERT INTO inbox_priorities (name, rank) VALUES (?1, ?2)", params![name, rank])?;
        }
        for issue in &self.issues {
            let number = issue.key.rsplit('-').next().unwrap_or_default();
            tx.execute(
                "INSERT INTO inbox (
                    issue_key, issue_id, summary, status_id, priority, priority_rank, sort_value, url,
                    first_seen, last_seen, notified, pinned, dismissed, issue_type, project, source,
                    taken_at, snoozed_until, gone_at, left_reason, resolution, last_change, changed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?12, ?13, ?14, 'jira', ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                params![
                    issue.key,
                    format!("20{number}"),
                    issue.summary,
                    issue.status.0,
                    issue.priority.0,
                    issue.priority.1,
                    issue.score,
                    format!("https://tracker.example/browse/{}", issue.key),
                    issue.first_seen,
                    issue.gone_at.unwrap_or(self.to.and_time(NaiveTime::MIN)).max(issue.first_seen),
                    issue.pinned,
                    issue.dismissed,
                    issue.issue_type,
                    PROJECT,
                    issue.taken_at,
                    issue.snoozed_until,
                    issue.gone_at,
                    issue.gone_at.map(|_| "resolved"),
                    issue.gone_at.map(|_| "Done"),
                    issue.change.map(|(change, _)| change),
                    issue.change.map(|(_, at)| at),
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }
}

impl Day {
    /// A finished day of the given shape; its hours depend on its date alone.
    fn past(date: NaiveDate, shape: Shape) -> Day {
        let mut rng = Rng::for_date(date, 1);
        let at = |hour: u32, minute: u32| date.and_time(NaiveTime::from_hms_opt(hour, minute, 0).expect("a time of day"));

        let start = at(8, 40) + Duration::seconds(rng.between(0, 60 * 60));
        let length = match shape {
            Shape::Long => Duration::minutes(rng.between(10 * 60 + 10, 10 * 60 + 50)),
            _ => Duration::minutes(rng.between(8 * 60 + 15, 9 * 60 + 25)),
        };
        let end = start + length;

        let mut pauses = Vec::new();
        // Lunch, every day.
        let lunch = at(12, 20) + Duration::seconds(rng.between(0, 40 * 60));
        pauses.push(Pause {
            start: lunch,
            end: lunch + Duration::seconds(rng.between(35 * 60, 60 * 60)),
            reason: None,
        });
        // A real break in the afternoon, most days.
        if rng.chance(60) {
            let coffee = at(15, 30) + Duration::seconds(rng.between(0, 40 * 60));
            pauses.push(Pause {
                start: coffee,
                end: coffee + Duration::seconds(rng.between(20 * 60, 32 * 60)),
                reason: None,
            });
        }
        if shape == Shape::Appointment {
            pauses.push(Pause {
                start: at(10, 15),
                end: at(11, 40),
                reason: Some("Dentist"),
            });
        }

        // The short ones: away for a few minutes, back again. They do not
        // split the day; they are what productivity is measured by.
        let (count, shortest, longest) = match shape {
            Shape::Scattered => (rng.between(13, 16), 9 * 60, 18 * 60),
            _ => (rng.between(3, 7), 2 * 60, 9 * 60),
        };
        let span = (end - start).num_seconds();
        let margin = Duration::minutes(6);
        let mut placed = 0;
        // Bounded: a crowded day takes what fits and stops asking.
        for _ in 0..count * 12 {
            if placed == count {
                break;
            }
            let from = start + Duration::seconds(rng.between(20 * 60, span - 30 * 60));
            let to = from + Duration::seconds(rng.between(shortest, longest));
            if pauses.iter().all(|pause| to + margin < pause.start || pause.end + margin < from) {
                pauses.push(Pause {
                    start: from,
                    end: to,
                    reason: None,
                });
                placed += 1;
            }
        }
        pauses.sort_by_key(|pause| pause.start);

        Day {
            date,
            start,
            end: Some(end),
            pauses,
        }
    }

    /// Today's day as it stands at `now`: the same morning a finished day
    /// would have had, cut where the clock is. `None` before the day has begun.
    fn running(date: NaiveDate, now: NaiveDateTime) -> Option<Day> {
        let whole = Day::past(date, Shape::Ordinary);
        if now < whole.start + Duration::minutes(15) {
            return None;
        }
        // Past the hour the day would have ended at, it has ended.
        if whole.end.is_some_and(|end| end <= now) {
            return Some(whole);
        }
        Some(Day {
            pauses: whole.pauses.into_iter().filter(|pause| pause.end < now).collect(),
            end: None,
            ..whole
        })
    }
}

/// Lays the backlog over the days, two pieces of work at a time, with the
/// meetings a week has. Returns the rows, and the issues in hand today and
/// finished before it - each with when it was taken.
#[allow(clippy::type_complexity)]
fn schedule(days: &[Day], now: NaiveDateTime) -> (Vec<TaskRow>, Vec<(u32, NaiveDateTime)>, Vec<(u32, NaiveDateTime, NaiveDateTime)>) {
    /// A piece of work under way.
    struct Running {
        chain: usize,
        work: &'static Work,
        done: u32,
        since: NaiveDateTime,
    }

    let mut rows = Vec::new();
    let mut backlog = BACKLOG.iter().cycle();
    let mut next_chain = 0;
    let mut running: Vec<Running> = Vec::new();
    let mut finished = Vec::new();

    for day in days {
        while running.len() < 2 {
            let work = backlog.next().expect("a cycle never ends");
            running.push(Running {
                chain: next_chain,
                work,
                done: 0,
                since: day.start + Duration::minutes(12),
            });
            next_chain += 1;
        }

        // Rows are written through the day, never after the moment the demo is read at.
        let last = day.end.unwrap_or(now).min(now);
        let mut at = day.start + Duration::minutes(25);
        let mut stamp = || {
            let this = at.min(last);
            at += Duration::minutes(95);
            this
        };

        match day.date.weekday() {
            Weekday::Mon if day.date.iso_week().week() % 2 == 0 => rows.push(meeting(&mut next_chain, stamp(), "Sprint planning")),
            Weekday::Fri if day.date.iso_week().week() % 2 == 1 => rows.push(meeting(&mut next_chain, stamp(), "Team retrospective")),
            Weekday::Tue | Weekday::Thu => rows.push(TaskRow {
                chain: {
                    next_chain += 1;
                    next_chain - 1
                },
                at: stamp(),
                name: "Code review".to_string(),
                comment: "",
                completeness: 100,
                issue_key: None,
                tags: &["review"],
            }),
            _ => {}
        }

        for piece in &mut running {
            piece.done += 1;
            let key = piece.work.issue.map(|number| format!("{PROJECT}-{number}"));
            rows.push(TaskRow {
                chain: piece.chain,
                at: stamp(),
                name: match &key {
                    Some(key) => format!("{key} {}", piece.work.name),
                    None => piece.work.name.to_string(),
                },
                comment: piece.work.comment,
                completeness: (piece.done * 100 / piece.work.days) as i32,
                issue_key: key,
                tags: piece.work.tags,
            });
        }
        // Today's work is in hand, however far it got: nothing is finished on a day still running.
        if day.end.is_some() {
            let day_end = day.end.unwrap_or(last);
            running.retain(|piece| {
                let over = piece.done >= piece.work.days;
                if over && let Some(number) = piece.work.issue {
                    finished.push((number, piece.since, day_end));
                }
                !over
            });
        }
    }

    let in_hand = running
        .iter()
        .filter_map(|piece| piece.work.issue.map(|number| (number, piece.since)))
        .collect();
    (rows, in_hand, finished)
}

fn meeting(next_chain: &mut usize, at: NaiveDateTime, name: &str) -> TaskRow {
    *next_chain += 1;
    TaskRow {
        chain: *next_chain - 1,
        at,
        name: name.to_string(),
        comment: "",
        completeness: 100,
        issue_key: None,
        tags: &["meeting"],
    }
}

/// The inbox at `now`: the issues in hand, the last one finished, and what
/// waits - in every state the list can show.
fn inbox(now: NaiveDateTime, in_hand: &[(u32, NaiveDateTime)], finished: &[(u32, NaiveDateTime, NaiveDateTime)]) -> Vec<Issue> {
    let summary_of = |number: u32| BACKLOG.iter().find(|work| work.issue == Some(number)).map(|work| work.name).unwrap_or_default();
    let mut issues = Vec::new();

    for (index, (number, since)) in in_hand.iter().enumerate() {
        issues.push(Issue {
            key: format!("{PROJECT}-{number}"),
            summary: summary_of(*number),
            status: STATUS_IN_PROGRESS,
            priority: if index == 0 { ("High", 2) } else { ("Medium", 3) },
            score: 7.5 - index as f64,
            issue_type: "Story",
            first_seen: *since - Duration::days(2),
            pinned: false,
            dismissed: false,
            taken_at: Some(*since),
            snoozed_until: None,
            gone_at: None,
            change: None,
        });
    }

    // The issues finished in the last two weeks: resolved in the tracker, so
    // they left the inbox - and a standup of this week or the last links them.
    for (number, since, done) in finished.iter().filter(|(_, _, done)| now - *done < Duration::days(14)) {
        issues.push(Issue {
            key: format!("{PROJECT}-{number}"),
            summary: summary_of(*number),
            status: STATUS_DONE,
            priority: ("Medium", 3),
            score: 5.5,
            issue_type: "Story",
            first_seen: *since - Duration::days(2),
            pinned: false,
            dismissed: false,
            taken_at: Some(*since),
            snoozed_until: None,
            gone_at: Some(*done + Duration::hours(14)),
            change: None,
        });
    }

    for (index, (number, summary, issue_type, priority, score)) in WAITING.iter().enumerate() {
        let hours_ago = |hours: i64| now - Duration::hours(hours);
        // The first three arrived within the day and read as NEW; the rest
        // have been waiting, one in each state the list knows.
        let first_seen = match index {
            0 => hours_ago(2),
            1 => hours_ago(5),
            2 => hours_ago(19),
            _ => hours_ago(24 * (3 + index as i64 * 2)),
        };
        issues.push(Issue {
            key: format!("{PROJECT}-{number}"),
            summary,
            status: if index == 4 { STATUS_IN_REVIEW } else { STATUS_TODO },
            priority: *priority,
            score: *score,
            issue_type,
            first_seen,
            pinned: index == 3,
            dismissed: index == 7,
            taken_at: None,
            snoozed_until: (index == 8).then(|| now + Duration::days(3)),
            gone_at: None,
            change: (index == 4).then(|| ("status→In Review", hours_ago(6))),
        });
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(date: (i32, u32, u32), time: (u32, u32)) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap().and_hms_opt(time.0, time.1, 0).unwrap()
    }

    /// A Wednesday afternoon.
    fn wednesday() -> NaiveDateTime {
        at((2026, 11, 25), (15, 10))
    }

    #[test]
    fn two_demos_of_one_moment_are_the_same_demo() {
        assert_eq!(Demo::generate(wednesday()), Demo::generate(wednesday()));
    }

    #[test]
    fn a_past_day_is_the_same_from_whichever_day_it_is_looked_at() {
        let earlier = Demo::generate(wednesday());
        let later = Demo::generate(wednesday() + Duration::days(1));
        let monday = at((2026, 11, 23), (0, 0)).date();
        let day = |demo: &Demo| demo.days.iter().find(|day| day.date == monday).cloned().expect("Monday is there");
        assert_eq!(day(&earlier), day(&later));
    }

    #[test]
    fn it_is_eight_weeks_of_weekdays_ending_today() {
        let demo = Demo::generate(wednesday());
        let today = wednesday().date();
        assert_eq!(
            demo.days.first().unwrap().date,
            at((2026, 10, 5), (0, 0)).date(),
            "the Monday seven weeks before this week's"
        );
        assert_eq!(demo.days.last().unwrap().date, today);
        assert!(demo.days.iter().all(|day| is_weekday(day.date)));
        // Thirty-eight weekdays in the window, two of them days of leave.
        assert_eq!(demo.days.len(), 36);
    }

    #[test]
    fn today_is_open_and_holds_nothing_from_the_future() {
        let now = wednesday();
        let demo = Demo::generate(now);
        let today = demo.days.last().unwrap();
        assert!(today.end.is_none(), "today's day is still running");
        assert!(today.start < now);
        assert!(today.pauses.iter().all(|pause| pause.end < now));
        assert!(demo.tasks.iter().all(|row| row.at <= now));
        // Every earlier day is closed.
        assert!(demo.days[..demo.days.len() - 1].iter().all(|day| day.end.is_some()));
    }

    #[test]
    fn a_morning_too_early_has_no_today_and_a_weekend_has_none_either() {
        let early = at((2026, 11, 25), (6, 0));
        assert!(Demo::generate(early).days.last().unwrap().date < early.date());
        let sunday = at((2026, 11, 29), (12, 0));
        assert_eq!(Demo::generate(sunday).days.last().unwrap().date, at((2026, 11, 27), (0, 0)).date());
    }

    #[test]
    fn pauses_lie_inside_their_day_and_never_overlap() {
        let demo = Demo::generate(wednesday());
        for day in &demo.days {
            let end = day.end.unwrap_or(wednesday());
            for pause in &day.pauses {
                assert!(day.start < pause.start && pause.start < pause.end && pause.end <= end, "{pause:?} in {day:?}");
            }
            for pair in day.pauses.windows(2) {
                assert!(pair[0].end < pair[1].start, "{pair:?}");
            }
        }
    }

    #[test]
    fn every_kind_of_day_is_there() {
        let demo = Demo::generate(wednesday());
        let long = |day: &Day| day.end.is_some_and(|end| end - day.start >= Duration::hours(10));
        assert!(demo.days.iter().any(long), "a long day");
        assert!(
            demo.days.iter().any(|day| day.pauses.iter().any(|pause| pause.reason.is_some())),
            "a pause recorded by hand"
        );
        assert!(demo.days.iter().any(|day| day.pauses.len() >= 12), "a scattered day");
    }

    #[test]
    fn a_task_runs_over_its_days_and_ends_done() {
        let demo = Demo::generate(wednesday());
        let rows: Vec<&TaskRow> = demo.tasks.iter().filter(|row| row.issue_key.as_deref() == Some("ORBIT-404")).collect();
        assert_eq!(rows.len(), 4, "four days of work");
        assert_eq!(rows.iter().map(|row| row.completeness).collect::<Vec<_>>(), vec![25, 50, 75, 100]);
        assert!(rows.iter().all(|row| row.chain == rows[0].chain));
        assert!(rows.windows(2).all(|pair| pair[0].at.date() < pair[1].at.date()));
    }

    #[test]
    fn the_inbox_shows_every_state_and_agrees_with_the_tasks() {
        let now = wednesday();
        let demo = Demo::generate(now);
        let fresh = demo.issues.iter().filter(|issue| now - issue.first_seen < Duration::hours(24)).count();
        assert_eq!(fresh, 3, "three arrived within the day");
        assert!(demo.issues.iter().any(|issue| issue.pinned));
        assert!(demo.issues.iter().any(|issue| issue.dismissed));
        assert!(demo.issues.iter().any(|issue| issue.snoozed_until.is_some()));
        assert!(demo.issues.iter().any(|issue| issue.gone_at.is_some()));
        assert!(demo.issues.iter().any(|issue| issue.change.is_some()));

        // What the inbox calls taken and still present is what today's tasks are about.
        let today = now.date();
        for issue in demo.issues.iter().filter(|issue| issue.taken_at.is_some() && issue.gone_at.is_none()) {
            let row = demo
                .tasks
                .iter()
                .find(|row| row.at.date() == today && row.issue_key.as_deref() == Some(&issue.key));
            assert!(
                row.is_some_and(|row| row.completeness < 100 || row.at.date() == today),
                "{} is in hand today",
                issue.key
            );
        }
        let keys: std::collections::HashSet<&String> = demo.issues.iter().map(|issue| &issue.key).collect();
        assert_eq!(keys.len(), demo.issues.len(), "one row per key");
    }

    #[test]
    fn the_demo_prefix_is_a_command_after_the_word() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(prefix(&args(&["kasl", "demo", "report"])), Some(1));
        assert_eq!(prefix(&args(&["kasl", "--plain", "demo", "sum", "--json"])), Some(2));
        assert_eq!(prefix(&args(&["kasl", "demo"])), None);
        assert_eq!(prefix(&args(&["kasl", "demo", "--reset"])), None);
        assert_eq!(prefix(&args(&["kasl", "task", "add", "--name", "demo"])), None);
        assert_eq!(prefix(&args(&["kasl"])), None);
    }
}
