use crate::db::pauses::Pauses;
use crate::libs::config::Config;
use crate::libs::view::View;
use chrono::{Duration, Local, NaiveDate};
use clap::Args;
use std::error::Error;

/// Command-line arguments for the `pauses` command.
#[derive(Debug, Args)]
pub struct PausesArgs {
    /// Date to fetch pauses for, in 'YYYY-MM-DD' format or 'today'.
    #[arg(long, short, default_value = "today", help = "Date to fetch pauses for (YYYY-MM-DD or 'today')")]
    date: String,
    /// Minimum pause duration in minutes (overrides config if provided).
    #[arg(long, short, help = "Minimum pause duration in minutes")]
    min_duration: Option<u64>,
}

/// Executes the `pauses` command to display pauses for a given date.
///
/// Fetches pause records from the database for the specified date, filtered by the minimum
/// duration, and displays them in a formatted table using the `View` module.
///
/// # Arguments
/// * `args` - The parsed command-line arguments containing the date and optional minimum duration.
///
/// # Returns
/// A `Result` indicating success or an error if database operations or date parsing fail.
pub async fn cmd(args: PausesArgs) -> Result<(), Box<dyn Error>> {
    // Parse the provided date string into a NaiveDate.
    let date = parse_date(&args.date)?;

    // Load configuration to retrieve default minimum pause duration.
    let config = Config::read()?;
    let min_duration = args.min_duration.unwrap_or(config.monitor.unwrap_or_default().min_pause_duration);

    // Fetch pauses for the specified date, filtered by minimum duration.
    let pauses = Pauses::new()?.fetch(date, min_duration)?;
   
    // Calculate total pause time
    let total_pause_time = pauses
        .iter()
        .filter_map(|p| p.duration)
        .fold(Duration::zero(), |acc, d| acc + d);
   
    // Display the pauses in a formatted table.
    println!("\nPauses for {}", date.format("%B %-d, %Y"));
    View::pauses(&pauses, total_pause_time)?;
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