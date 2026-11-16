//! The trash: what a remove command took away, kept so `kasl undo` can put
//! it back.
//!
//! A removed row leaves its table for good and its content moves here, as
//! JSON, with the rows that went with it - the tag links a deleted task or
//! tag took along. The live tables hold only live rows, so no query has to
//! remember to skip a deleted one (ADR 0013).
//!
//! One command is one operation: `kasl task remove 4 5 6` is put back whole.
//! Operations older than [`RETENTION_DAYS`] are dropped the next time
//! something is removed - a deleted row is kept long enough to notice the
//! mistake, not forever.
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use kasl::db::trash::Trash;
//!
//! if let Some(removal) = Trash::new()?.restore_last()? {
//!     println!("put back {} rows of `kasl {}`", removal.items.len(), removal.kind.command());
//! }
//! # Ok(())
//! # }
//! ```

use crate::db::db::Db;
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// How long a removed row can still be put back.
pub const RETENTION_DAYS: i64 = 30;

pub(crate) const SCHEMA_TRASH: &str = "CREATE TABLE IF NOT EXISTS trash (
    id INTEGER PRIMARY KEY,
    operation INTEGER NOT NULL,
    removed_at TIMESTAMP NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('task', 'pause', 'tag', 'template')),
    source TEXT NOT NULL,
    row TEXT NOT NULL
)";

/// The table holding the links between tasks and tags.
const LINKS: &str = "task_tags";

const NEXT_OPERATION: &str = "SELECT COALESCE(MAX(operation), 0) + 1 FROM trash";
const INSERT_ROW: &str = "INSERT INTO trash (operation, removed_at, kind, source, row) VALUES (?1, datetime('now'), ?2, ?3, ?4)";
const SELECT_ROWS: &str = "SELECT operation, removed_at, kind, source, row FROM trash ORDER BY operation DESC, id ASC";
const LAST_OPERATION: &str = "SELECT MAX(operation) FROM trash";
const DELETE_OPERATION: &str = "DELETE FROM trash WHERE operation = ?1";
const SELECT_LINK: &str = "SELECT 1 FROM task_tags WHERE task_id = ?1 AND tag_id = ?2";
// A link comes back only between a task and a tag that are both there: the
// tag may have been removed for good since, and the chain may be someone
// else's by now.
const RESTORE_LINK: &str = "INSERT OR IGNORE INTO task_tags (task_id, tag_id)
    SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM tags WHERE id = ?2) AND EXISTS (SELECT 1 FROM tasks WHERE task_id = ?1)";

/// What a remove command removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Task,
    Pause,
    Tag,
    Template,
}

impl Kind {
    /// The table the rows live in.
    pub fn table(self) -> &'static str {
        match self {
            Kind::Task => "tasks",
            Kind::Pause => "pauses",
            Kind::Tag => "tags",
            Kind::Template => "task_templates",
        }
    }

    /// The command that removes them, without the program name.
    pub fn command(self) -> &'static str {
        match self {
            Kind::Task => "task remove",
            Kind::Pause => "pauses remove",
            Kind::Tag => "tag remove",
            Kind::Template => "template remove",
        }
    }

    /// The word for one of them, as stored and as `--json` says it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Task => "task",
            Kind::Pause => "pause",
            Kind::Tag => "tag",
            Kind::Template => "template",
        }
    }

    fn parse(name: &str) -> Result<Kind> {
        [Kind::Task, Kind::Pause, Kind::Tag, Kind::Template]
            .into_iter()
            .find(|kind| kind.name() == name)
            .ok_or_else(|| anyhow!("the trash holds a row of an unknown kind: {name}"))
    }

    /// Whether two rows of this kind cannot share a name.
    fn named_uniquely(self) -> bool {
        matches!(self, Kind::Tag | Kind::Template)
    }
}

/// One removed row, as it is listed and as it came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The row's id: the one it had, or the one it has after coming back.
    pub id: i64,

    /// The id it had when removed, when it came back under another - a row
    /// added since has taken the old one.
    pub renumbered_from: Option<i64>,

    /// What a person calls it: a task's name, a pause's hours, a tag's name.
    pub label: String,

    /// The day it belongs to; a tag or a template belongs to none.
    pub date: Option<NaiveDate>,
}

/// Everything one remove command took away.
#[derive(Debug, Clone)]
pub struct Removal {
    pub kind: Kind,
    pub removed_at: DateTime<Utc>,
    pub items: Vec<Item>,
}

/// Removes the rows with these ids from the table of `kind`, keeping them -
/// and the tag links that go with them - in the trash as one operation.
///
/// The only way a row of these tables is deleted, so there is no removal
/// that `kasl undo` does not know. Unknown ids are not counted; the return
/// is the number of rows removed.
pub fn remove(conn: &Connection, kind: Kind, ids: &[i64]) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;
    tx.execute(SCHEMA_TRASH, [])?;
    tx.execute(&format!("DELETE FROM trash WHERE removed_at < datetime('now', '-{RETENTION_DAYS} days')"), [])?;

    let rows = select_by(&tx, kind.table(), "id", ids)?;
    if rows.is_empty() {
        return Ok(0);
    }

    // The links are read before the delete and compared after it: a tag
    // removed takes all of its links by cascade, a task takes its chain's
    // only when the chain's last row goes.
    let links = match kind {
        Kind::Task => {
            let mut chains: Vec<i64> = rows.iter().filter_map(|row| row.get("task_id").and_then(Value::as_i64)).collect();
            chains.sort_unstable();
            chains.dedup();
            select_by(&tx, LINKS, "task_id", &chains)?
        }
        Kind::Tag => select_by(&tx, LINKS, "tag_id", ids)?,
        Kind::Pause | Kind::Template => Vec::new(),
    };

    let removed = tx.execute(
        &format!("DELETE FROM {} WHERE id IN ({})", kind.table(), placeholders(ids.len())),
        params_from_iter(ids),
    )?;

    let operation: i64 = tx.query_row(NEXT_OPERATION, [], |row| row.get(0))?;
    for row in &rows {
        tx.execute(
            INSERT_ROW,
            params![operation, kind.name(), kind.table(), Value::Object(row.clone()).to_string()],
        )?;
    }
    for link in &links {
        let (task_id, tag_id) = link_ends(link)?;
        let still_there = tx.query_row(SELECT_LINK, params![task_id, tag_id], |_| Ok(())).optional()?.is_some();
        if !still_there {
            tx.execute(INSERT_ROW, params![operation, kind.name(), LINKS, Value::Object(link.clone()).to_string()])?;
        }
    }

    tx.commit()?;
    Ok(removed)
}

/// Access to the trash for `kasl undo`.
pub struct Trash {
    pub conn: Connection,
}

impl Trash {
    /// Opens the database and ensures the trash table exists.
    pub fn new() -> Result<Self> {
        let db = Db::new()?;
        db.conn.execute(SCHEMA_TRASH, [])?;
        Ok(Trash { conn: db.conn })
    }

    /// What can be put back, the latest removal first.
    pub fn list(&self) -> Result<Vec<Removal>> {
        let mut removals: Vec<Removal> = Vec::new();
        let mut current: Option<i64> = None;

        for stored in self.stored()? {
            if current != Some(stored.operation) {
                current = Some(stored.operation);
                removals.push(Removal {
                    kind: stored.kind,
                    removed_at: stored.removed_at,
                    items: Vec::new(),
                });
            }
            if stored.source == stored.kind.table()
                && let Some(removal) = removals.last_mut()
            {
                let id = row_id(&stored.row)?;
                removal.items.push(item(stored.kind, id, None, &stored.row));
            }
        }

        Ok(removals)
    }

    /// Puts back what the latest remove command took away; `None` when the
    /// trash is empty.
    ///
    /// A row returns under the id it had. When a row added since has taken
    /// that id, it returns under a new one, and what pointed at the old id -
    /// the rest of a task's chain removed with it, its tag links - follows.
    /// A tag or a template whose name is in use again is not put back over
    /// the newer one: the call fails and the trash keeps the removal.
    pub fn restore_last(&mut self) -> Result<Option<Removal>> {
        let Some(operation) = self.conn.query_row(LAST_OPERATION, [], |row| row.get::<_, Option<i64>>(0))? else {
            return Ok(None);
        };
        let stored: Vec<Stored> = self.stored()?.into_iter().filter(|row| row.operation == operation).collect();
        let Some(first) = stored.first() else {
            return Ok(None);
        };
        let (kind, removed_at) = (first.kind, first.removed_at);
        let table = kind.table();

        let tx = self.conn.transaction()?;
        let live_columns = columns(&tx, table)?;
        let mut renumbered: HashMap<i64, i64> = HashMap::new();
        let mut items = Vec::new();

        for stored in stored.iter().filter(|row| row.source == table) {
            let mut row = stored.row.clone();
            let was = row_id(&row)?;

            if kind.named_uniquely()
                && let Some(name) = row.get("name").and_then(Value::as_str)
                && exists(&tx, table, "name", &SqlValue::Text(name.to_string()))?
            {
                bail!("a {} named '{}' exists now; rename or remove it, then run `kasl undo` again", kind.name(), name);
            }

            let taken = exists(&tx, table, "id", &SqlValue::Integer(was))?;
            if taken {
                row.remove("id");
            }
            // A chain is named after its first row: when that row came back
            // under a new id, the rows after it follow.
            let heads_its_chain = kind == Kind::Task && row.get("task_id").and_then(Value::as_i64) == Some(was);
            if kind == Kind::Task
                && let Some(chain) = row.get("task_id").and_then(Value::as_i64)
                && let Some(moved) = renumbered.get(&chain)
            {
                row.insert("task_id".to_string(), Value::from(*moved));
            }

            let id = insert(&tx, table, &live_columns, &row).with_context(|| format!("cannot put back {} {}", kind.name(), was))?;
            if taken {
                renumbered.insert(was, id);
                if heads_its_chain {
                    tx.execute("UPDATE tasks SET task_id = ?1 WHERE id = ?1", params![id])?;
                }
            }
            items.push(item(kind, id, taken.then_some(was), &stored.row));
        }

        for stored in stored.iter().filter(|row| row.source == LINKS) {
            let (task_id, tag_id) = link_ends(&stored.row)?;
            let moved = |id: i64, of: Kind| if kind == of { *renumbered.get(&id).unwrap_or(&id) } else { id };
            tx.execute(RESTORE_LINK, params![moved(task_id, Kind::Task), moved(tag_id, Kind::Tag)])?;
        }

        tx.execute(DELETE_OPERATION, params![operation])?;
        tx.commit()?;

        Ok(Some(Removal { kind, removed_at, items }))
    }

    /// Every stored row, the latest operation first, each operation in the
    /// order it was written.
    fn stored(&self) -> Result<Vec<Stored>> {
        let mut statement = self.conn.prepare(SELECT_ROWS)?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, NaiveDateTime>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;

        let mut stored = Vec::new();
        for row in rows {
            let (operation, removed_at, kind, source, json) = row?;
            let Value::Object(row) = serde_json::from_str(&json).context("the trash holds a row that is not JSON")? else {
                bail!("the trash holds a row that is not an object");
            };
            stored.push(Stored {
                operation,
                removed_at: removed_at.and_utc(),
                kind: Kind::parse(&kind)?,
                source,
                row,
            });
        }
        Ok(stored)
    }
}

/// One row of the trash table.
struct Stored {
    operation: i64,
    removed_at: DateTime<Utc>,
    kind: Kind,
    /// The table the row was in: the kind's own, or the tag links.
    source: String,
    row: Map<String, Value>,
}

/// The rows of `table` whose `column` is one of `values`, each as a JSON
/// object keyed by column name - whatever columns the table has today.
fn select_by(conn: &Connection, table: &str, column: &str, values: &[i64]) -> Result<Vec<Map<String, Value>>> {
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = conn.prepare(&format!(
        "SELECT * FROM {table} WHERE {column} IN ({}) ORDER BY rowid",
        placeholders(values.len())
    ))?;
    let names: Vec<String> = statement.column_names().into_iter().map(str::to_string).collect();

    let rows = statement.query_map(params_from_iter(values), |row| {
        let mut object = Map::new();
        for (index, name) in names.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null | ValueRef::Blob(_) => Value::Null,
                ValueRef::Integer(number) => Value::from(number),
                ValueRef::Real(number) => Value::from(number),
                ValueRef::Text(text) => Value::from(String::from_utf8_lossy(text).into_owned()),
            };
            object.insert(name.clone(), value);
        }
        Ok(object)
    })?;

    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Inserts a stored row into `table` and returns its id.
///
/// Only the columns the table still has are written: a row removed before a
/// migration dropped a column comes back without it.
fn insert(conn: &Connection, table: &str, live_columns: &[String], row: &Map<String, Value>) -> Result<i64> {
    let (names, values): (Vec<String>, Vec<SqlValue>) = row
        .iter()
        .filter(|(name, _)| live_columns.contains(name))
        .map(|(name, value)| {
            let value = match value {
                Value::Number(number) => number
                    .as_i64()
                    .map(SqlValue::Integer)
                    .or(number.as_f64().map(SqlValue::Real))
                    .unwrap_or(SqlValue::Null),
                Value::String(text) => SqlValue::Text(text.clone()),
                Value::Bool(flag) => SqlValue::Integer(*flag as i64),
                _ => SqlValue::Null,
            };
            (format!("\"{name}\""), value)
        })
        .unzip();

    conn.execute(
        &format!("INSERT INTO {table} ({}) VALUES ({})", names.join(", "), placeholders(names.len())),
        params_from_iter(values),
    )?;
    Ok(conn.last_insert_rowid())
}

/// The names of the columns `table` has now.
fn columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare("SELECT name FROM pragma_table_info(?1)")?;
    let names = statement.query_map([table], |row| row.get::<_, String>(0))?;
    names.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

/// Whether `table` has a row whose `column` equals `value`.
fn exists(conn: &Connection, table: &str, column: &str, value: &SqlValue) -> Result<bool> {
    let found = conn
        .query_row(&format!("SELECT 1 FROM {table} WHERE {column} = ?1"), [value], |_| Ok(()))
        .optional()?;
    Ok(found.is_some())
}

fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

fn row_id(row: &Map<String, Value>) -> Result<i64> {
    row.get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("the trash holds a row without an id"))
}

fn link_ends(link: &Map<String, Value>) -> Result<(i64, i64)> {
    let end = |name: &str| link.get(name).and_then(Value::as_i64);
    end("task_id")
        .zip(end("tag_id"))
        .ok_or_else(|| anyhow!("the trash holds a tag link without its ends"))
}

/// The listing's view of a stored row.
fn item(kind: Kind, id: i64, renumbered_from: Option<i64>, row: &Map<String, Value>) -> Item {
    let text = |name: &str| row.get(name).and_then(Value::as_str).unwrap_or_default();
    // Stamps are stored as `YYYY-MM-DD HH:MM:SS`.
    let day = |stamp: &str| stamp.get(..10).and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok());
    let clock = |stamp: &str| stamp.get(11..16).unwrap_or("?").to_string();

    let (label, date) = match kind {
        Kind::Task => (text("name").to_string(), day(text("timestamp"))),
        Kind::Pause => {
            let end = row.get("end").and_then(Value::as_str).map(clock).unwrap_or_else(|| "open".to_string());
            (format!("{}-{}", clock(text("start")), end), day(text("start")))
        }
        Kind::Tag | Kind::Template => (text("name").to_string(), None),
    };

    Item {
        id,
        renumbered_from,
        label,
        date,
    }
}
