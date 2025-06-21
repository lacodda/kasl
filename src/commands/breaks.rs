use crate::db::breaks::Breaks;
use crate::libs::view::View;
use chrono::{Local, NaiveDate};
use clap::Args;
use std::error::Error;

// Arguments for the breaks command.
#[derive(Debug, Args)]
pub struct BreaksArgs {
    #[arg(long, short, default_value = "today", help = "Date to fetch breaks for (YYYY-MM-DD or 'today')")]
    date: String,
    #[arg(long, short, default_value_t = 20, help = "Minimum break duration in minutes")]
    min_duration: u64,
}

// Runs the breaks command to display breaks for a given date.
pub async fn cmd(args: BreaksArgs) -> Result<(), Box<dyn Error>> {
    let date = parse_date(&args.date)?;
    let breaks = Breaks::new()?.fetch(date, args.min_duration)?;
    View::breaks(&breaks)?;
    Ok(())
}

// Parses the date string into a NaiveDate.
fn parse_date(date_str: &str) -> Result<NaiveDate, Box<dyn Error>> {
    if date_str.to_lowercase() == "today" {
        Ok(Local::now().date_naive())
    } else {
        Ok(NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?)
    }
}
