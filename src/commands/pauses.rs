//! Display recorded breaks and pauses command.
//!
//! This command provides detailed views of automatically detected and manually
//! recorded breaks during work sessions. It helps users understand their break
//! patterns and verify the accuracy of automatic pause detection.

use crate::db::pauses::Pauses;
use crate::libs::config::Config;
use crate::libs::messages::Message;
use crate::libs::view::View;
use crate::msg_print;
use anyhow::Result;
use chrono::{Duration, Local, NaiveDate};
use clap::Args;

/// Command-line arguments for the pauses command.
///
/// This command allows users to view breaks for any date with optional
/// filtering by duration to focus on significant pauses.
#[derive(Debug, Args)]
pub struct PausesArgs {
    /// Date to fetch pauses for
    ///
    /// Accepts dates in 'YYYY-MM-DD' format or the special keyword 'today'
    /// for the current date. This allows users to review break patterns
    /// for any historical date.
    ///
    /// # Examples
    /// - `today` - Current date
    /// - `2025-01-15` - Specific date
    /// - `2025-12-25` - Christmas day
    #[arg(long, short, default_value = "today", help = "Date to fetch pauses for (YYYY-MM-DD or 'today')")]
    date: String,

    /// Minimum pause duration filter in minutes
    ///
    /// When specified, only pauses longer than this duration will be displayed.
    /// This overrides the default minimum pause duration from configuration
    /// and is useful for:
    /// - Filtering out brief interruptions
    /// - Focusing on significant breaks
    /// - Comparing different threshold values
    ///
    /// If not specified, uses the configured `min_pause_duration` setting.
    #[arg(long, short, help = "Minimum pause duration in minutes")]
    min_duration: Option<u64>,
}

/// Executes the pauses command to display breaks for a given date.
///
/// This function retrieves and displays pause records from the database,
/// applying duration filtering and presenting the results in a formatted table.
/// It provides both individual pause details and summary statistics.
///
/// ## Display Format
///
/// The output includes:
/// - **Pause List**: Each pause with start time, end time, and duration
/// - **Total Time**: Sum of all pause durations for the day
/// - **Pause Count**: Number of breaks recorded
///
/// ## Duration Filtering
///
/// Pauses are filtered by minimum duration to remove noise:
/// 1. Uses command-line `--min-duration` if provided
/// 2. Falls back to config file `min_pause_duration` setting
/// 3. Defaults to reasonable threshold if no configuration exists
///
/// ## Data Sources
///
/// Pause data comes from:
/// - **Automatic Detection**: Monitor-recorded inactivity periods
/// - **Manual Adjustments**: User-added pauses via adjust command
/// - **Time Corrections**: Modified pause times from manual adjustments
///
/// # Arguments
///
/// * `args` - Parsed command-line arguments containing date and filter options
///
/// # Returns
///
/// Returns `Ok(())` on successful display, or an error if date parsing fails
/// or database operations encounter issues.
///
/// # Examples
///
/// ```bash
/// # Show today's pauses
/// kasl pauses
///
/// # Show pauses for specific date
/// kasl pauses --date 2025-01-15
///
/// # Filter to show only pauses longer than 30 minutes
/// kasl pauses --min-duration 30
///
/// # Combine date and duration filtering
/// kasl pauses --date 2025-01-15 --min-duration 10
/// ```
///
/// # Error Scenarios
///
/// - Invalid date format in `--date` argument
/// - Database connection failures
/// - Configuration file read errors
/// - Permission issues accessing pause records
pub async fn cmd(args: PausesArgs) -> Result<()> {
    // Parse the provided date string into a structured date
    let date = parse_date(&args.date)?;

    // Load configuration to get default minimum pause duration
    let config = Config::read()?;
    let min_duration = args.min_duration.unwrap_or(config.monitor.unwrap_or_default().min_pause_duration);

    // Fetch pause records from database with duration filtering
    let pauses = Pauses::new()?.fetch(date, min_duration)?;

    // Calculate total pause time for summary statistics
    let total_pause_time = pauses.iter().filter_map(|p| p.duration).fold(Duration::zero(), |acc, d| acc + d);

    // Display formatted results with date header
    msg_print!(Message::PausesTitle(date.format("%B %-d, %Y").to_string()), true);
    View::pauses(&pauses, total_pause_time)?;

    Ok(())
}

/// Parses a date string into a structured date value.
///
/// This helper function handles both the special 'today' keyword and
/// explicit date strings in ISO format (YYYY-MM-DD). It provides
/// user-friendly date input parsing with clear error messages.
///
/// # Arguments
///
/// * `date_str` - The date string to parse, either 'today' or 'YYYY-MM-DD'
///
/// # Returns
///
/// Returns the parsed `NaiveDate` on success, or an error if the date
/// string is invalid or unparseable.
///
/// # Supported Formats
///
/// - `today` (case-insensitive) - Returns current local date
/// - `YYYY-MM-DD` - ISO 8601 date format (e.g., `2025-01-15`)
///
/// # Examples
///
/// ```rust
/// let today = parse_date("today")?;
/// let specific = parse_date("2025-12-25")?;
/// ```
///
/// # Error Cases
///
/// - Malformed date strings (e.g., `2025-13-45`)
/// - Invalid date formats (e.g., `01/15/2025`)
/// - Non-existent dates (e.g., `2025-02-30`)
fn parse_date(date_str: &str) -> Result<NaiveDate> {
    if date_str.to_lowercase() == "today" {
        Ok(Local::now().date_naive())
    } else {
        Ok(NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?)
    }
}
