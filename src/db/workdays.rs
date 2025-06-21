use crate::db::db::Db;
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OptionalExtension, Result};
use std::error::Error;

const SCHEMA_WORKDAYS: &str = "CREATE TABLE IF NOT EXISTS workdays (
    id INTEGER PRIMARY KEY,
    date DATE NOT NULL UNIQUE,
    start TIMESTAMP NOT NULL,
    end TIMESTAMP
);";
const INSERT_START: &str = "INSERT INTO workdays (date, start) VALUES (?1, datetime(CURRENT_TIMESTAMP, 'localtime'))";
const UPDATE_END: &str = "UPDATE workdays SET end = datetime(CURRENT_TIMESTAMP, 'localtime') WHERE date = ?1";
const SELECT_BY_DATE: &str = "SELECT id, date, start, end FROM workdays WHERE date = ?1";
const SELECT_BY_MONTH: &str = "SELECT id, date, start, end FROM workdays WHERE strftime('%Y-%m', date) = strftime('%Y-%m', ?1)";

#[derive(Debug, Clone)]
pub struct Workday {
    pub id: i32,
    pub date: NaiveDate,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
}

pub struct Workdays {
    conn: Connection,
}

impl Workdays {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let db = Db::new()?;
        db.conn.execute(SCHEMA_WORKDAYS, [])?;
        Ok(Workdays { conn: db.conn })
    }

    pub fn insert_start(&mut self, date: NaiveDate) -> Result<(), Box<dyn Error>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        // Check if workday already exists for the date
        if self.fetch(date)?.is_none() {
            self.conn.execute(INSERT_START, [&date_str])?;
        }
        Ok(())
    }

    pub fn insert_end(&mut self, date: NaiveDate) -> Result<(), Box<dyn Error>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        self.conn.execute(UPDATE_END, [&date_str])?;
        Ok(())
    }

    pub fn fetch(&mut self, date: NaiveDate) -> Result<Option<Workday>, Box<dyn Error>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let workday = self
            .conn
            .query_row(SELECT_BY_DATE, [&date_str], |row| {
                Ok(Workday {
                    id: row.get(0)?,
                    date: NaiveDate::parse_from_str(&row.get::<_, String>(1)?, "%Y-%m-%d").unwrap(),
                    start: NaiveDateTime::parse_from_str(&row.get::<_, String>(2)?, "%Y-%m-%d %H:%M:%S").unwrap(),
                    end: row
                        .get::<_, Option<String>>(3)?
                        .map(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").unwrap()),
                })
            })
            .optional()?;
        Ok(workday)
    }

    pub fn fetch_month(&mut self, date: NaiveDate) -> Result<Vec<Workday>, Box<dyn Error>> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut stmt = self.conn.prepare(SELECT_BY_MONTH)?;
        let workday_iter = stmt.query_map([&date_str], |row| {
            Ok(Workday {
                id: row.get(0)?,
                date: NaiveDate::parse_from_str(&row.get::<_, String>(1)?, "%Y-%m-%d").unwrap(),
                start: NaiveDateTime::parse_from_str(&row.get::<_, String>(2)?, "%Y-%m-%d %H:%M:%S").unwrap(),
                end: row
                    .get::<_, Option<String>>(3)?
                    .map(|s| NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").unwrap()),
            })
        })?;
        let mut workdays = Vec::new();
        for workday in workday_iter {
            workdays.push(workday?);
        }
        Ok(workdays)
    }
}
