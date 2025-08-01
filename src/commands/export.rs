//! Data export command for external analysis and backup.
//!
//! This module provides comprehensive data export functionality, supporting
//! multiple output formats and data types. It enables users to extract their
//! work data for external analysis, backup purposes, or integration with
//! other tools and systems.
//!
//! ## Supported Export Formats
//!
//! - **CSV**: Comma-separated values for spreadsheet applications
//! - **JSON**: Structured data for programmatic processing
//! - **Excel**: Native spreadsheet format with formatting and multiple sheets
//!
//! ## Data Types
//!
//! - **Reports**: Daily work reports with intervals, tasks, and productivity
//! - **Tasks**: Task records with completion status and metadata
//! - **Summary**: Monthly summaries with aggregate statistics
//! - **All**: Complete data export including all available information

use crate::{
    libs::{
        export::{ExportData, ExportFormat, Exporter},
        messages::Message,
    },
    msg_info,
};
use anyhow::Result;
use chrono::{Local, NaiveDate};
use clap::Args;
use std::path::PathBuf;

/// Command-line arguments for the export command.
///
/// The export command provides flexible options for data extraction,
/// supporting different formats, data types, and output destinations.
#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Type of data to export
    ///
    /// Specifies which category of information to include in the export:
    /// - **report**: Daily work report with intervals and productivity
    /// - **tasks**: Task records with completion status and metadata
    /// - **summary**: Monthly summary with aggregate statistics
    /// - **all**: Complete data export including all available information
    ///
    /// Each data type provides different levels of detail and is suitable
    /// for different analysis purposes.
    #[arg(value_enum, default_value = "report")]
    data: ExportData,

    /// Output format for the exported data
    ///
    /// Controls the structure and format of the exported file:
    /// - **csv**: Comma-separated values, compatible with Excel and other spreadsheet tools
    /// - **json**: Structured JSON data, ideal for programmatic processing
    /// - **excel**: Native Excel format with formatting, charts, and multiple worksheets
    ///
    /// Format selection affects both file structure and available features.
    #[arg(short, long, value_enum, default_value = "csv")]
    format: ExportFormat,

    /// Custom output file path
    ///
    /// When specified, the export will be saved to this exact location.
    /// If not provided, a default filename will be generated based on:
    /// - Current timestamp for uniqueness
    /// - Selected data type for clarity
    /// - Chosen format for proper file extension
    ///
    /// Example default: `kasl_export_20250115_143022.csv`
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Target date for data export
    ///
    /// Specifies which date's data to export. Accepts:
    /// - `today`: Current date (default)
    /// - `YYYY-MM-DD`: Specific date in ISO format
    ///
    /// For summary exports, this determines the month to summarize.
    /// For daily reports and tasks, this specifies the exact date.
    #[arg(short, long, default_value = "today")]
    date: String,
}

/// Executes the data export command.
///
/// This function orchestrates the complete export process:
/// 1. **Date Parsing**: Converts date string to structured format
/// 2. **Exporter Initialization**: Sets up output format and destination
/// 3. **Data Processing**: Delegates to appropriate export handler
/// 4. **File Generation**: Creates output file with requested data
/// 5. **User Feedback**: Provides confirmation and file location
///
/// ## Export Process
///
/// The export process varies by data type but generally follows:
/// 1. **Data Retrieval**: Fetch relevant records from database
/// 2. **Data Transformation**: Convert to export-friendly format
/// 3. **Format Application**: Apply CSV, JSON, or Excel formatting
/// 4. **File Writing**: Save to specified or default location
/// 5. **Validation**: Verify export completeness and integrity
///
/// ## Error Handling
///
/// The export process includes robust error handling for:
/// - Invalid date formats or ranges
/// - Database connectivity issues
/// - File system permission problems
/// - Data format conversion errors
/// - Output file write failures
///
/// # Arguments
///
/// * `args` - Parsed command-line arguments specifying export parameters
///
/// # Returns
///
/// Returns `Ok(())` on successful export completion, or an error if
/// any step in the export process fails.
///
/// # Examples
///
/// ```bash
/// # Export today's report as CSV
/// kasl export report --format csv
///
/// # Export tasks from specific date as JSON
/// kasl export tasks --format json --date 2025-01-15
///
/// # Export monthly summary to Excel with custom filename
/// kasl export summary --format excel --output monthly_report.xlsx
///
/// # Export all data for backup purposes
/// kasl export all --format json --output backup_2025_01.json
/// ```
///
/// # Output Files
///
/// Generated files include:
/// - **Metadata**: Export timestamp, data range, format version
/// - **Data Records**: Requested information in chosen format
/// - **Summary Statistics**: Totals, averages, and key metrics
/// - **Format-Specific Features**: Charts (Excel), structured nesting (JSON)
pub async fn cmd(args: ExportArgs) -> Result<()> {
    let date = parse_date(&args.date)?;

    msg_info!(Message::ExportingData(format!("{:?}", args.data), format!("{:?}", args.format)));

    // Initialize exporter with format and output configuration
    let exporter = Exporter::new(args.format, args.output);

    // Delegate to appropriate export handler based on data type
    exporter.export(args.data, date).await?;

    Ok(())
}

/// Parses a date string supporting both 'today' and ISO format.
///
/// This utility function provides consistent date parsing across the export
/// command, handling both user-friendly keywords and explicit date specifications.
///
/// ## Supported Formats
///
/// - **today** (case-insensitive): Returns current local date
/// - **YYYY-MM-DD**: ISO 8601 date format (e.g., 2025-01-15)
///
/// ## Use Cases
///
/// Different date specifications serve different purposes:
/// - `today`: Quick exports of current work data
/// - Specific dates: Historical analysis, backup creation, data migration
/// - Recent dates: Weekly or monthly review processes
///
/// # Arguments
///
/// * `date_str` - Date string to parse, either 'today' or 'YYYY-MM-DD'
///
/// # Returns
///
/// Returns the parsed `NaiveDate` on success, or an error if the date
/// string is malformed or represents an invalid date.
///
/// # Error Scenarios
///
/// - Malformed date strings (e.g., `2025-13-45`, `invalid-date`)
/// - Wrong date formats (e.g., `01/15/2025`, `15-01-2025`)
/// - Non-existent dates (e.g., `2025-02-30`, `2025-04-31`)
/// - Out-of-range values (e.g., month > 12, day > 31)
///
/// # Examples
///
/// ```rust
/// let today = parse_date("today")?;           // Current date
/// let christmas = parse_date("2025-12-25")?;  // Specific holiday
/// let start_year = parse_date("2025-01-01")?; // Year beginning
/// ```
fn parse_date(date_str: &str) -> Result<NaiveDate> {
    if date_str.to_lowercase() == "today" {
        Ok(Local::now().date_naive())
    } else {
        Ok(NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?)
    }
}
