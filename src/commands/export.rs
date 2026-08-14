//! Data export command for external analysis and backup.
//!
//! Provides comprehensive data export functionality supporting multiple output formats and data types for external analysis, backup, and integration.
//!
//! ## Usage
//!
//! ```bash
//! # Export tasks to CSV
//! kasl export tasks --format csv
//!
//! # Export today's report to Excel
//! kasl export report --format xlsx
//!
//! # Export with custom filename
//! kasl export tasks --format json --output my_tasks.json
//! ```

use crate::{
    libs::{
        config::Config,
        export::{ExportData, ExportFormat, Exporter},
        formatter::parse_date,
        messages::Message,
    },
    msg_info,
};
use anyhow::Result;
use chrono::NaiveDate;
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
    /// Each data type provides different levels of detail and is suitable
    /// for different analysis purposes.
    #[arg(value_enum, default_value = "report")]
    data: ExportData,

    /// Output format for the exported data
    ///
    /// Controls the structure and format of the exported file:
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

    /// Render the daily report as an hourly (SiServer-style) breakdown
    ///
    /// When enabled, the report is exported as a per-hour grid: each row
    /// represents one hour of the workday with a description of the work
    /// performed, and "Перерыв" is written for hours (or parts of hours) that
    /// fall within a break or pause.
    ///
    /// This option only affects Excel report exports (`report --format excel`);
    /// it is ignored for other data types and formats.
    #[arg(long)]
    hourly: bool,
}

/// Executes the export: parses the date, resolves the output path, and
/// hands off to the [`Exporter`].
///
/// ```bash
/// kasl export report --format csv
/// kasl export tasks --format json --date 2025-01-15
/// kasl export summary --format excel --output monthly_report.xlsx
/// kasl export all --format json --output backup_2025_01.json
/// ```
pub async fn cmd(args: ExportArgs) -> Result<()> {
    let date = parse_date(&args.date)?;

    msg_info!(Message::ExportingData(format!("{:?}", args.data), format!("{:?}", args.format)));

    // Resolve the output path: an explicit --output always wins; otherwise, for
    // report exports, fall back to the configured directory and file-name template.
    let output = match args.output.clone() {
        Some(path) => Some(path),
        None => resolve_report_output(args.data, args.format, date)?,
    };

    // Initialize exporter with format and output configuration
    let exporter = Exporter::new(args.format, output).hourly(args.hourly);

    // Delegate to appropriate export handler based on data type
    exporter.export(args.data, date).await?;

    Ok(())
}

/// Resolves a default output path for report exports from configuration.
///
/// This is only applied to [`ExportData::Report`] exports when no explicit
/// `--output` was provided and a report output directory is configured. The
/// file name is built from the configured template (defaulting to
/// `daily_report_{date}{seq}`), where `{date}` is the report date and `{seq}`
/// is a per-day sequence suffix (empty for the first file of the day, then
/// `_2`, `_3`, … for subsequent files). The chosen path is guaranteed not to
/// overwrite an existing file.
///
/// Returns `Ok(None)` to defer to the exporter's built-in default naming when
/// the export is not a report or no report directory is configured.
fn resolve_report_output(data: ExportData, format: ExportFormat, date: NaiveDate) -> Result<Option<PathBuf>> {
    if !matches!(data, ExportData::Report) {
        return Ok(None);
    }

    let report_config = match Config::read()?.report {
        Some(config) => config,
        None => return Ok(None),
    };

    let output_dir = match report_config.output_dir {
        Some(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => return Ok(None),
    };

    std::fs::create_dir_all(&output_dir)?;

    let template = report_config
        .filename_template
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "daily_report_{date}{seq}".to_string());

    let extension = match format {
        ExportFormat::Csv => "csv",
        ExportFormat::Json => "json",
        ExportFormat::Excel => "xlsx",
    };
    let date_str = date.format("%Y-%m-%d").to_string();

    // Pick the first non-existing file for the day, appending _2, _3, … as needed.
    for sequence in 1.. {
        let seq_suffix = if sequence == 1 { String::new() } else { format!("_{}", sequence) };
        let stem = template.replace("{date}", &date_str).replace("{seq}", &seq_suffix);
        let candidate = output_dir.join(format!("{}.{}", stem, extension));
        if !candidate.exists() {
            return Ok(Some(candidate));
        }
    }

    unreachable!("sequence iterator is unbounded")
}
