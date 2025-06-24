use crate::db::breaks::Breaks;
use crate::libs::config::Config;
use crate::libs::view::View;
use chrono::{Local, NaiveDate};
use clap::Args;
use std::error::Error;

/// Command-line arguments for the `breaks` command.
#[derive(Debug, Args)]
pub struct BreaksArgs {
    /// Date to fetch breaks for, in 'YYYY-MM-DD' format or 'today'.
    #[arg(long, short, default_value = "today", help = "Date to fetch breaks for (YYYY-MM-DD or 'today')")]
    date: String,
    /// Minimum break duration in minutes (overrides config if provided).
    #[arg(long, short, help = "Minimum break duration in minutes")]
    min_duration: Option<u64>,
}

/// Executes the `breaks` command to display breaks for a given date.
///
/// Fetches break records from the database for the specified date, filtered by the minimum
/// break duration, and displays them in a formatted table using the `View` module.
///
/// # Arguments
/// * `args` - The parsed command-line arguments containing the date and optional minimum duration.
///
/// # Returns
/// A `Result` indicating success or an error if database operations or date parsing fail.
pub async fn cmd(args: BreaksArgs) -> Result<(), Box<dyn Error>> {
    // Parse the provided date string into a NaiveDate.
    let date = parse_date(&args.date)?;

    // Load configuration to retrieve default minimum break duration.
    let config = Config::read()?;
    let min_duration = args.min_duration.unwrap_or(config.monitor.unwrap_or_default().min_break_duration);

    // Fetch breaks for the specified date, filtered by minimum duration.
    let breaks = Breaks::new()?.fetch(date, min_duration)?;

    // Display the breaks in a formatted table.
    println!("\nBreaks for {}", date.format("%B %-d, %Y"));
    View::breaks(&breaks)?;
    Ok(())
}

/// Parses a date string into a `NaiveDate`.
///
/// Supports 'today' (case-insensitive) for the current date or a date string in 'YYYY-MM-DD' format.
///
/// # Arguments
/// * `date_str` - The date string to parse.
///
/// # Returns
/// A `Result` containing the parsed `NaiveDate` or an error if parsing fails.
fn parse_date(date_str: &str) -> Result<NaiveDate, Box<dyn Error>> {
    if date_str.to_lowercase() == "today" {
        Ok(Local::now().date_naive())
    } else {
        Ok(NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?)
    }
}
