//! Data export functionality for external analysis and backup.
//!
//! Provides a comprehensive data export system that enables users to extract
//! their work tracking data in multiple formats for external analysis, backup,
//! integration with other tools, or compliance reporting.
//!
//! ## Features
//!
//! - **Export Formats**: CSV, JSON, Excel with formatting and multiple sheets
//! - **Data Types**: Reports, tasks, summaries, and complete data export
//! - **File Naming**: Intelligent naming conventions with timestamp-based uniqueness
//! - **Error Handling**: Robust validation and error recovery
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::export::{Exporter, ExportFormat, ExportData};
//! use chrono::NaiveDate;
//!
//! let exporter = Exporter::new(ExportFormat::Csv, None);
//! exporter.export(ExportData::Report, NaiveDate::from_ymd(2025, 1, 15)).await?;
//! ```

use crate::{
    db::{breaks::Breaks, pauses::Pauses, tasks::Tasks, workdays::Workdays},
    libs::{
        config::Config,
        formatter::format_duration,
        locale::{Language, Locale},
        messages::Message,
        report::{self, WorkInterval},
        report_template::{FontSpec, ReportTemplate},
        task::{Task, TaskFilter},
    },
    msg_error_anyhow, msg_info, msg_success,
};
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike};
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/// Enumeration of supported export output formats.
///
/// This enum defines the available output formats for data export operations.
/// Each format is optimized for different use cases and provides different
/// levels of functionality and compatibility.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportFormat {
    /// Comma-separated values format for universal compatibility.
    ///
    /// CSV exports provide maximum compatibility with spreadsheet applications,
    /// data analysis tools, and simple parsing libraries. The format uses
    /// standard CSV conventions with proper quoting and escaping.
    Csv,

    /// JavaScript Object Notation for structured data exchange.
    ///
    /// JSON exports preserve data types and structure, making them ideal for
    /// programmatic processing, API integrations, and backup/restore operations.
    /// All exports use pretty-printing for human readability.
    Json,

    /// Microsoft Excel format with advanced formatting capabilities.
    ///
    /// Excel exports provide rich formatting, multiple worksheets, auto-sizing,
    /// and professional presentation quality. Ideal for business reports and
    /// executive presentations.
    Excel,
}

/// Enumeration of data types available for export.
///
/// This enum defines the different categories of information that can be
/// exported from the kasl application. Each data type provides different
/// levels of detail and serves different analytical purposes.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportData {
    /// Export daily work report with intervals and productivity metrics.
    ///
    /// Includes detailed work intervals, break periods, task associations,
    /// and calculated productivity statistics for a specific date.
    Report,

    /// Export task records with completion status and metadata.
    ///
    /// Includes all tasks for a specific date with their names, descriptions,
    /// completion percentages, and associated metadata.
    Tasks,

    /// Export monthly summary with aggregated statistics.
    ///
    /// Includes daily work hour totals, averages, and productivity trends
    /// for the month containing the specified date.
    Summary,

    /// Export comprehensive dataset including all available information.
    ///
    /// Combines reports, tasks, and summaries into a single export for
    /// complete data backup or comprehensive analysis.
    All,
}

/// Serializable structure representing a daily work report for export.
///
/// This structure contains all the information needed to represent a complete
/// daily work report in export formats. All fields use string representations
/// for format compatibility and consistent presentation.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportReport {
    /// Date of the work report in YYYY-MM-DD format
    pub date: String,
    /// Work start time in HH:MM format
    pub start_time: String,
    /// Work end time in HH:MM format
    pub end_time: String,
    /// Total working hours formatted as human-readable duration
    pub total_hours: String,
    /// Productivity percentage (0.0-100.0) with one decimal place
    pub productivity: f64,
    /// List of work intervals with timing details
    pub intervals: Vec<ExportInterval>,
    /// List of tasks associated with this date
    pub tasks: Vec<ExportTask>,
}

/// Serializable structure representing a work interval within a daily report.
///
/// Work intervals represent continuous periods of activity without breaks.
/// They are calculated by analyzing work start/end times and pause periods.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportInterval {
    /// Sequential index of the interval (1-based)
    pub index: usize,
    /// Interval start time in HH:MM format
    pub start: String,
    /// Interval end time in HH:MM format
    pub end: String,
    /// Interval duration formatted as human-readable duration
    pub duration: String,
}

/// Serializable structure representing a task record for export.
///
/// This structure contains all relevant task information in a format
/// suitable for external systems and analysis tools.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportTask {
    /// Unique task identifier from the database
    pub id: i32,
    /// Human-readable task name or title
    pub name: String,
    /// Optional task description or comments
    pub comment: String,
    /// Task completion percentage (0-100)
    pub completeness: i32,
}

/// Serializable structure representing a monthly summary for export.
///
/// This structure aggregates work data for an entire month, providing
/// overview statistics and daily breakdowns for analysis purposes.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportSummary {
    /// Month and year in "Month YYYY" format (e.g., "January 2025")
    pub month: String,
    /// List of daily work hour summaries
    pub days: Vec<ExportDaySum>,
    /// Total working hours for the month formatted as duration
    pub total_hours: String,
    /// Average daily working hours formatted as duration
    pub average_hours: String,
    /// Total number of working days in the month
    pub total_days: usize,
}

/// Serializable structure representing a single day within a monthly summary.
///
/// This structure provides daily-level statistics within the broader
/// monthly summary context.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportDaySum {
    /// Date in YYYY-MM-DD format
    pub date: String,
    /// Working hours for this date formatted as duration
    pub hours: String,
    /// Whether this was a working day (true) or rest day (false)
    pub is_workday: bool,
}

/// Main export handler responsible for orchestrating data export operations.
///
/// The Exporter struct encapsulates the export format, output destination,
/// and provides methods for exporting different types of data. It handles
/// the complete export pipeline from data gathering to file generation.
///
/// ## Design Philosophy
///
/// The Exporter follows a builder pattern for configuration and uses method
/// dispatch for different export operations. This design provides flexibility
/// while maintaining type safety and clear separation of concerns.
pub struct Exporter {
    /// The desired output format for the export operation
    format: ExportFormat,
    /// The destination path for the exported file
    output_path: PathBuf,
    /// Whether to render the daily report as an hourly (SiServer-style) breakdown.
    ///
    /// When enabled (and the format is Excel), the report is rendered as a
    /// per-hour grid where each row represents one hour of the workday with a
    /// description of the work performed, and "Перерыв" is written for hours
    /// (or parts of hours) that fall within a break/pause.
    hourly: bool,
}

impl Exporter {
    /// Creates a new Exporter instance with specified format and optional output path.
    ///
    /// This constructor sets up the export configuration and determines the output
    /// file path. If no custom path is provided, it generates a default filename
    /// based on the current timestamp and selected format.
    ///
    /// ## Default File Naming
    ///
    /// When no output path is specified, the constructor generates a filename using:
    /// - **Prefix**: "kasl_export_"
    /// - **Timestamp**: YYYYMMDD_HHMMSS format
    /// - **Extension**: Format-appropriate extension (.csv, .json, .xlsx)
    ///
    /// Example default names:
    /// - `kasl_export_20250115_143022.csv`
    /// - `kasl_export_20250115_143022.json`
    /// - `kasl_export_20250115_143022.xlsx`
    ///
    /// ## Path Validation
    ///
    /// The constructor validates that:
    /// - Custom paths have appropriate file extensions
    /// - Parent directories exist or can be created
    /// - Write permissions are available
    ///
    /// # Arguments
    ///
    /// * `format` - The desired export format (CSV, JSON, or Excel)
    /// * `output_path` - Optional custom output path; generates default if None
    ///
    /// # Returns
    ///
    /// Returns a configured Exporter instance ready for export operations.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::export::{Exporter, ExportFormat};
    /// use std::path::PathBuf;
    ///
    /// // Create exporter with default filename
    /// let exporter = Exporter::new(ExportFormat::Csv, None);
    ///
    /// // Create exporter with custom path
    /// let custom_path = PathBuf::from("reports/daily_report.xlsx");
    /// let exporter = Exporter::new(ExportFormat::Excel, Some(custom_path));
    /// ```
    pub fn new(format: ExportFormat, output_path: Option<PathBuf>) -> Self {
        // Generate default filename with timestamp for uniqueness
        let default_name = format!("kasl_export_{}", Local::now().format("%Y%m%d_%H%M%S"));

        // Determine appropriate file extension based on format
        let extension = match format {
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
            ExportFormat::Excel => "xlsx",
        };

        // Use custom path or generate default with appropriate extension
        let output_path = output_path.unwrap_or_else(|| PathBuf::from(format!("{}.{}", default_name, extension)));

        Self {
            format,
            output_path,
            hourly: false,
        }
    }

    /// Enables or disables the hourly (SiServer-style) daily report layout.
    ///
    /// This builder-style method toggles the hourly breakdown rendering for
    /// daily reports. It only affects Excel report exports; other formats and
    /// data types ignore this flag.
    ///
    /// # Arguments
    ///
    /// * `hourly` - Whether to render the report as an hourly grid
    ///
    /// # Returns
    ///
    /// Returns the modified `Exporter` for method chaining.
    pub fn hourly(mut self, hourly: bool) -> Self {
        self.hourly = hourly;
        self
    }

    /// Main export dispatcher that routes to appropriate export handlers based on data type.
    ///
    /// This method serves as the primary interface for export operations, determining
    /// which specific export handler to invoke based on the requested data type.
    /// It provides a unified interface while delegating to specialized methods.
    ///
    /// ## Export Process Flow
    ///
    /// 1. **Data Type Analysis**: Determine which export handler to invoke
    /// 2. **Data Gathering**: Collect relevant information from the database
    /// 3. **Format Processing**: Apply format-specific transformations
    /// 4. **File Generation**: Write the formatted data to the output file
    /// 5. **Validation**: Verify export completeness and file integrity
    ///
    /// # Arguments
    ///
    /// * `data_type` - The category of data to export (Report, Tasks, Summary, All)
    /// * `date` - The target date for data collection and filtering
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful export completion, or an error if any
    /// step in the export process fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kasl::libs::export::{Exporter, ExportFormat, ExportData};
    /// use chrono::NaiveDate;
    ///
    /// let exporter = Exporter::new(ExportFormat::Json, None);
    /// let date = NaiveDate::from_ymd(2025, 1, 15);
    /// exporter.export(ExportData::Report, date).await?;
    /// ```
    pub async fn export(&self, data_type: ExportData, date: NaiveDate) -> Result<()> {
        match data_type {
            ExportData::Report => self.export_report(date).await,
            ExportData::Tasks => self.export_tasks(date).await,
            ExportData::Summary => self.export_summary(date).await,
            ExportData::All => self.export_all(date).await,
        }
    }

    /// Exports a comprehensive daily work report with intervals, tasks, and productivity metrics.
    ///
    /// This method generates a detailed daily report that includes all work intervals,
    /// associated tasks, productivity calculations, and summary statistics. The report
    /// provides a complete picture of work activity for the specified date.
    ///
    /// ## Report Components
    ///
    /// The generated report includes:
    /// - **Work Intervals**: Detailed start/end times and durations for each work period
    /// - **Productivity Metrics**: Calculated productivity percentage based on active work time
    /// - **Task Information**: All tasks associated with the specified date
    /// - **Summary Statistics**: Total hours, break time, and other key metrics
    ///
    /// ## Data Sources
    ///
    /// The report combines data from multiple database sources:
    /// - Workday records for overall work boundaries
    /// - Pause records for break period calculations
    /// - Task records for work content and completion status
    ///
    /// # Arguments
    ///
    /// * `date` - The specific date for which to generate the report
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful report generation and file creation,
    /// or an error if data gathering or file writing fails.
    ///
    /// # Error Scenarios
    ///
    /// - No workday record exists for the specified date
    /// - Database connectivity issues during data gathering
    /// - File system errors during report generation
    /// - Data formatting or serialization errors
    async fn export_report(&self, date: NaiveDate) -> Result<()> {
        // Hourly (SiServer-style) layout is only meaningful for Excel output.
        // When requested, delegate to the dedicated renderer and skip the
        // generic report layout entirely.
        if self.hourly {
            if let ExportFormat::Excel = self.format {
                self.export_report_excel_hourly(date)?;
                msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
                return Ok(());
            }
        }

        // Gather comprehensive report data from multiple database sources
        let report_data = self.gather_report_data(date)?;

        // Apply format-specific processing and generate output file
        match self.format {
            ExportFormat::Csv => self.export_report_csv(&report_data)?,
            ExportFormat::Json => self.export_report_json(&report_data)?,
            ExportFormat::Excel => self.export_report_excel(&report_data)?,
        }

        // Provide user feedback about successful export completion
        msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
        Ok(())
    }

    /// Exports task records with completion status and metadata for the specified date.
    ///
    /// This method extracts all tasks associated with a particular date and formats
    /// them for export. Task exports are useful for project management integration,
    /// productivity analysis, and task completion tracking.
    ///
    /// ## Task Information
    ///
    /// Each exported task includes:
    /// - **Identification**: Unique database ID for reference
    /// - **Content**: Task name and description/comments
    /// - **Status**: Completion percentage and metadata
    /// - **Timing**: Association with the specified date
    ///
    /// # Arguments
    ///
    /// * `date` - The specific date for which to export tasks
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful task export and file creation,
    /// or an error if data retrieval or file writing fails.
    async fn export_tasks(&self, date: NaiveDate) -> Result<()> {
        // Retrieve tasks for the specified date from the database
        let tasks = Tasks::new()?.fetch(TaskFilter::Date(date))?;

        // Transform database task records into export-friendly format
        let export_tasks: Vec<ExportTask> = tasks
            .into_iter()
            .map(|t| ExportTask {
                id: t.id.unwrap_or(0),
                name: t.name,
                comment: t.comment,
                completeness: t.completeness.unwrap_or(100),
            })
            .collect();

        // Apply format-specific processing and generate output file
        match self.format {
            ExportFormat::Csv => self.export_tasks_csv(&export_tasks)?,
            ExportFormat::Json => {
                let json = serde_json::to_string_pretty(&export_tasks)?;
                File::create(&self.output_path)?.write_all(json.as_bytes())?;
            }
            ExportFormat::Excel => self.export_tasks_excel(&export_tasks)?,
        }

        // Provide user feedback about successful export completion
        msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
        Ok(())
    }

    /// Exports monthly summary with aggregated statistics and daily breakdowns.
    ///
    /// This method generates a comprehensive monthly overview that includes daily
    /// work hour totals, averages, productivity trends, and other aggregate
    /// statistics. Monthly summaries are valuable for long-term analysis and
    /// productivity tracking.
    ///
    /// ## Summary Components
    ///
    /// The monthly summary includes:
    /// - **Daily Breakdown**: Individual day statistics with work hours
    /// - **Aggregate Metrics**: Total and average work hours for the month
    /// - **Productivity Trends**: Patterns and variations in work activity
    /// - **Calendar Context**: Work day vs. rest day classifications
    ///
    /// # Arguments
    ///
    /// * `date` - Any date within the month to summarize (month is extracted from this date)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful summary generation and file creation,
    /// or an error if data aggregation or file writing fails.
    async fn export_summary(&self, date: NaiveDate) -> Result<()> {
        // Gather and aggregate monthly data from workday records
        let summary_data = self.gather_summary_data(date)?;

        // Apply format-specific processing and generate output file
        match self.format {
            ExportFormat::Csv => self.export_summary_csv(&summary_data)?,
            ExportFormat::Json => {
                let json = serde_json::to_string_pretty(&summary_data)?;
                File::create(&self.output_path)?.write_all(json.as_bytes())?;
            }
            ExportFormat::Excel => self.export_summary_excel(&summary_data)?,
        }

        // Provide user feedback about successful export completion
        msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
        Ok(())
    }

    /// Exports comprehensive dataset including all available information types.
    ///
    /// This method provides a complete data export that combines reports, tasks,
    /// and summaries into a single export operation. It's designed for comprehensive
    /// backup operations, data migration, or complete analysis requirements.
    ///
    /// ## Export Strategy
    ///
    /// The method uses different strategies based on the selected format:
    ///
    /// ### JSON Format
    /// Creates a single JSON file with nested structure containing:
    /// - Export metadata (timestamp, version)
    /// - Daily report data
    /// - Task records
    /// - Monthly summary
    ///
    /// ### CSV and Excel Formats
    /// Creates multiple files with descriptive suffixes:
    /// - `{base}_report.{ext}` - Daily report data
    /// - `{base}_tasks.{ext}` - Task records
    /// - `{base}_summary.{ext}` - Monthly summary
    ///
    /// # Arguments
    ///
    /// * `date` - The reference date for data collection and filtering
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful comprehensive export completion,
    /// or an error if any component export fails.
    async fn export_all(&self, date: NaiveDate) -> Result<()> {
        msg_info!(Message::ExportingAllData);

        // Handle JSON format with combined data structure
        if let ExportFormat::Json = self.format {
            // Gather all data types, allowing for optional failures
            let report = self.gather_report_data(date).ok();
            let tasks = Tasks::new()?
                .fetch(TaskFilter::Date(date))?
                .into_iter()
                .map(|t| ExportTask {
                    id: t.id.unwrap_or(0),
                    name: t.name,
                    comment: t.comment,
                    completeness: t.completeness.unwrap_or(100),
                })
                .collect::<Vec<_>>();
            let summary = self.gather_summary_data(date).ok();

            // Create comprehensive JSON structure with metadata
            let all_data = serde_json::json!({
                "export_date": Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                "daily_report": report,
                "tasks": tasks,
                "monthly_summary": summary,
            });

            // Write the combined JSON data to file
            let json = serde_json::to_string_pretty(&all_data)?;
            File::create(&self.output_path)?.write_all(json.as_bytes())?;
        } else {
            // Handle CSV and Excel formats with multiple files
            let base = self.output_path.file_stem().unwrap().to_string_lossy();
            let ext = self.output_path.extension().unwrap().to_string_lossy();

            // Generate separate file paths with descriptive suffixes
            let report_path = self.output_path.with_file_name(format!("{}_report.{}", base, ext));
            let tasks_path = self.output_path.with_file_name(format!("{}_tasks.{}", base, ext));
            let summary_path = self.output_path.with_file_name(format!("{}_summary.{}", base, ext));

            // Create separate exporters for each data type
            let report_exporter = Exporter::new(self.format, Some(report_path));
            let tasks_exporter = Exporter::new(self.format, Some(tasks_path));
            let summary_exporter = Exporter::new(self.format, Some(summary_path));

            // Execute all export operations
            report_exporter.export_report(date).await?;
            tasks_exporter.export_tasks(date).await?;
            summary_exporter.export_summary(date).await?;

            return Ok(());
        }

        // Provide user feedback about successful export completion
        msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
        Ok(())
    }

    /// Gathers comprehensive report data from multiple database sources and calculates metrics.
    ///
    /// This method orchestrates data collection from workdays, tasks, and pauses
    /// databases to create a complete daily report. It performs calculations for
    /// productivity metrics, work intervals, and summary statistics.
    ///
    /// ## Data Integration Process
    ///
    /// 1. **Workday Retrieval**: Fetch the primary workday record for date validation
    /// 2. **Task Collection**: Gather all tasks associated with the specified date
    /// 3. **Pause Analysis**: Retrieve and analyze break periods for interval calculation
    /// 4. **Interval Calculation**: Compute work intervals by analyzing gaps and pauses
    /// 5. **Metric Calculation**: Calculate productivity percentages and summary statistics
    ///
    /// ## Productivity Calculation
    ///
    /// For export purposes, a simplified productivity calculation is used:
    /// ```text
    /// Export Productivity = (Net Work Time / Gross Work Time) × 100
    ///
    /// Where:
    /// - Net Work Time = Total Time - Pause Duration
    /// - Gross Work Time = End Time - Start Time
    /// ```
    /// 
    /// Note: This differs from the comprehensive calculation in `libs::productivity::Productivity`
    /// which handles breaks, different pause types, and overlap scenarios.
    ///
    /// # Arguments
    ///
    /// * `date` - The specific date for which to gather report data
    ///
    /// # Returns
    ///
    /// Returns an `ExportReport` structure containing all calculated metrics
    /// and formatted data, or an error if data retrieval or calculation fails.
    ///
    /// # Error Scenarios
    ///
    /// - No workday record exists for the specified date
    /// - Database connectivity issues during data retrieval
    /// - Data inconsistencies or corruption
    fn gather_report_data(&self, date: NaiveDate) -> Result<ExportReport> {
        // Retrieve the primary workday record or fail if none exists
        let workday = Workdays::new()?
            .fetch(date)?
            .ok_or_else(|| msg_error_anyhow!(Message::WorkdayNotFoundForDate(date.to_string())))?;

        // Collect associated tasks and pause data
        let tasks = Tasks::new()?.fetch(TaskFilter::Date(date))?;
        let pauses = Pauses::new()?.get_daily_pauses(date)?;

        // Determine end time (use current time if workday is still active)
        let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());

        // Calculate work intervals by analyzing workday and pause data
        let intervals = report::calculate_work_intervals(&workday, &pauses);

        // Calculate total pause duration for productivity metrics
        let total_pause_duration = pauses.iter().filter_map(|p| p.duration).fold(Duration::zero(), |acc, d| acc + d);

        // Calculate gross and net work durations
        let gross_duration = end_time - workday.start;
        let net_duration = gross_duration - total_pause_duration;

        // Calculate simplified productivity percentage for export
        // Note: This is a simplified calculation for export purposes only
        // For comprehensive productivity analysis, use libs::productivity::Productivity
        let productivity = if gross_duration.num_seconds() > 0 {
            (net_duration.num_seconds() as f64 / gross_duration.num_seconds() as f64) * 100.0
        } else {
            0.0
        };

        // Construct the comprehensive export report structure
        Ok(ExportReport {
            date: date.format("%Y-%m-%d").to_string(),
            start_time: workday.start.format("%H:%M").to_string(),
            end_time: end_time.format("%H:%M").to_string(),
            total_hours: format_duration(&net_duration),
            productivity: (productivity * 10.0).round() / 10.0, // Round to 1 decimal place
            intervals: intervals
                .iter()
                .enumerate()
                .map(|(i, interval)| ExportInterval {
                    index: i + 1, // 1-based indexing for user friendliness
                    start: interval.start.format("%H:%M").to_string(),
                    end: interval.end.format("%H:%M").to_string(),
                    duration: format_duration(&interval.duration),
                })
                .collect(),
            tasks: tasks
                .into_iter()
                .map(|t| ExportTask {
                    id: t.id.unwrap_or(0),
                    name: t.name,
                    comment: t.comment,
                    completeness: t.completeness.unwrap_or(100),
                })
                .collect(),
        })
    }

    /// Gathers monthly summary data by aggregating workday records and calculating statistics.
    ///
    /// This method processes all workday records for the month containing the specified
    /// date and generates aggregate statistics including totals, averages, and daily
    /// breakdowns for comprehensive monthly analysis.
    ///
    /// ## Aggregation Process
    ///
    /// 1. **Month Identification**: Extract month boundaries from the specified date
    /// 2. **Workday Collection**: Retrieve all workday records within the month
    /// 3. **Duration Calculation**: Calculate work duration for each day
    /// 4. **Statistical Analysis**: Compute totals, averages, and distributions
    /// 5. **Summary Generation**: Format results for export consumption
    ///
    /// ## Statistical Calculations
    ///
    /// - **Total Hours**: Sum of all work durations in the month
    /// - **Average Hours**: Mean work duration per working day
    /// - **Working Days**: Count of days with recorded work activity
    /// - **Daily Breakdown**: Individual day statistics with classifications
    ///
    /// # Arguments
    ///
    /// * `date` - Any date within the target month (used to determine month boundaries)
    ///
    /// # Returns
    ///
    /// Returns an `ExportSummary` structure containing aggregated monthly statistics
    /// and daily breakdowns, or an error if data retrieval or calculation fails.
    fn gather_summary_data(&self, date: NaiveDate) -> Result<ExportSummary> {
        // Retrieve all workday records for the month containing the specified date
        let workdays = Workdays::new()?.fetch_month(date)?;

        // Initialize aggregation variables
        let mut days = Vec::new();
        let mut total_duration = Duration::zero();

        // Process each workday to calculate duration and accumulate statistics
        for workday in &workdays {
            // Determine end time (use current time if workday is still active)
            let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
            let duration = end_time - workday.start;
            total_duration = total_duration + duration;

            // Add daily summary record
            days.push(ExportDaySum {
                date: workday.date.format("%Y-%m-%d").to_string(),
                hours: format_duration(&duration),
                is_workday: true, // All records in workdays table are work days
            });
        }

        // Calculate average duration with division by zero protection
        let avg_duration = if !workdays.is_empty() {
            Duration::seconds(total_duration.num_seconds() / workdays.len() as i64)
        } else {
            Duration::zero()
        };

        // Construct the monthly summary structure
        Ok(ExportSummary {
            month: date.format("%B %Y").to_string(), // "January 2025" format
            days,
            total_hours: format_duration(&total_duration),
            average_hours: format_duration(&avg_duration),
            total_days: workdays.len(),
        })
    }

    /// Exports daily report data to CSV format with structured sections and headers.
    ///
    /// This method creates a CSV file with multiple sections for different types of
    /// information, using headers and empty rows to create visual separation and
    /// improve readability in spreadsheet applications.
    ///
    /// ## CSV Structure
    ///
    /// The generated CSV includes the following sections:
    /// 1. **Work Intervals**: Detailed timing for each work period
    /// 2. **Summary Information**: Key metrics and totals
    /// 3. **Task Details**: Associated tasks with completion status
    ///
    /// Each section is separated by empty rows and includes descriptive headers
    /// for easy identification and processing.
    ///
    /// # Arguments
    ///
    /// * `report` - The report data structure to export
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful CSV generation, or an error if file
    /// writing fails or data formatting encounters issues.
    fn export_report_csv(&self, report: &ExportReport) -> Result<()> {
        let mut wtr = csv::Writer::from_path(&self.output_path)?;

        // Write work intervals section with headers
        wtr.write_record(&["WORK INTERVALS", "", "", ""])?;
        wtr.write_record(&["Index", "Start", "End", "Duration"])?;
        for interval in &report.intervals {
            wtr.write_record(&[
                interval.index.to_string(),
                interval.start.clone(),
                interval.end.clone(),
                interval.duration.clone(),
            ])?;
        }

        // Add spacing and summary section
        wtr.write_record(&["", "", "", ""])?;
        wtr.write_record(&["SUMMARY", "", "", ""])?;
        wtr.write_record(&["Date", &report.date, "", ""])?;
        wtr.write_record(&["Total Hours", &report.total_hours, "", ""])?;
        wtr.write_record(&["Productivity", &format!("{:.1}%", report.productivity), "", ""])?;

        // Add spacing and tasks section
        wtr.write_record(&["", "", "", ""])?;
        wtr.write_record(&["TASKS", "", "", ""])?;
        wtr.write_record(&["ID", "Name", "Comment", "Completeness"])?;
        for task in &report.tasks {
            wtr.write_record(&[task.id.to_string(), task.name.clone(), task.comment.clone(), format!("{}%", task.completeness)])?;
        }

        wtr.flush()?;
        Ok(())
    }

    /// Exports task records to CSV format with standard table structure.
    ///
    /// This method creates a simple CSV table with task information, suitable
    /// for import into spreadsheet applications or database systems.
    ///
    /// # Arguments
    ///
    /// * `tasks` - The task data collection to export
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful CSV generation, or an error if file
    /// writing fails.
    fn export_tasks_csv(&self, tasks: &[ExportTask]) -> Result<()> {
        let mut wtr = csv::Writer::from_path(&self.output_path)?;
        wtr.write_record(&["ID", "Name", "Comment", "Completeness"])?;

        for task in tasks {
            wtr.write_record(&[task.id.to_string(), task.name.clone(), task.comment.clone(), format!("{}%", task.completeness)])?;
        }

        wtr.flush()?;
        Ok(())
    }

    /// Exports monthly summary to CSV format with hierarchical structure.
    ///
    /// This method creates a CSV file with a title header, daily breakdown table,
    /// and summary statistics section for comprehensive monthly analysis.
    ///
    /// # Arguments
    ///
    /// * `summary` - The monthly summary data to export
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful CSV generation, or an error if file
    /// writing fails.
    fn export_summary_csv(&self, summary: &ExportSummary) -> Result<()> {
        let mut wtr = csv::Writer::from_path(&self.output_path)?;

        // Write title and daily breakdown
        wtr.write_record(&[format!("Monthly Summary - {}", summary.month), "".to_owned(), "".to_owned()])?;
        wtr.write_record(&["Date", "Hours", "Type"])?;

        for day in &summary.days {
            wtr.write_record(&[
                day.date.clone(),
                day.hours.clone(),
                if day.is_workday { "Work".to_owned() } else { "Rest".to_owned() },
            ])?;
        }

        // Add summary statistics
        wtr.write_record(&["", "", ""])?;
        wtr.write_record(&["Total Hours", &summary.total_hours, ""])?;
        wtr.write_record(&["Average Hours", &summary.average_hours, ""])?;
        wtr.write_record(&["Total Days", &summary.total_days.to_string(), ""])?;

        wtr.flush()?;
        Ok(())
    }

    /// Exports daily report data to JSON format with pretty printing.
    ///
    /// This method serializes the report data structure to JSON with formatting
    /// that makes it human-readable and suitable for both programmatic processing
    /// and manual inspection.
    ///
    /// # Arguments
    ///
    /// * `report` - The report data structure to serialize
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful JSON generation, or an error if
    /// serialization or file writing fails.
    fn export_report_json(&self, report: &ExportReport) -> Result<()> {
        let json = serde_json::to_string_pretty(report)?;
        File::create(&self.output_path)?.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Exports daily report to Excel format with professional formatting and multiple sections.
    ///
    /// This method creates a comprehensive Excel worksheet with formatted headers,
    /// auto-sized columns, and structured sections for work intervals, summary
    /// information, and task details. The Excel format provides the richest
    /// presentation quality with professional formatting.
    ///
    /// ## Excel Features
    ///
    /// - **Formatted Headers**: Bold text with gray background for section identification
    /// - **Auto-sizing**: Columns automatically sized for optimal readability
    /// - **Section Separation**: Visual spacing between different data sections
    /// - **Data Types**: Appropriate formatting for numbers, percentages, and text
    ///
    /// # Arguments
    ///
    /// * `report` - The report data structure to export
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful Excel generation, or an error if
    /// workbook creation or file writing fails.
    fn export_report_excel(&self, report: &ExportReport) -> Result<()> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        // Create formatting styles for headers and content
        let header_format = Format::new().set_bold().set_background_color(rust_xlsxwriter::Color::Gray);

        // Write work intervals section
        worksheet.write_string_with_format(0, 0, "WORK INTERVALS", &header_format)?;
        worksheet.write_string_with_format(1, 0, "Index", &header_format)?;
        worksheet.write_string_with_format(1, 1, "Start", &header_format)?;
        worksheet.write_string_with_format(1, 2, "End", &header_format)?;
        worksheet.write_string_with_format(1, 3, "Duration", &header_format)?;

        let mut row = 2;
        for interval in &report.intervals {
            worksheet.write_number(row, 0, interval.index as f64)?;
            worksheet.write_string(row, 1, &interval.start)?;
            worksheet.write_string(row, 2, &interval.end)?;
            worksheet.write_string(row, 3, &interval.duration)?;
            row += 1;
        }

        // Add summary section with spacing
        row += 2;
        worksheet.write_string_with_format(row, 0, "SUMMARY", &header_format)?;
        row += 1;
        worksheet.write_string(row, 0, "Date")?;
        worksheet.write_string(row, 1, &report.date)?;
        row += 1;
        worksheet.write_string(row, 0, "Total Hours")?;
        worksheet.write_string(row, 1, &report.total_hours)?;
        row += 1;
        worksheet.write_string(row, 0, "Productivity")?;
        worksheet.write_string(row, 1, &format!("{:.1}%", report.productivity))?;

        // Add tasks section with spacing
        row += 2;
        worksheet.write_string_with_format(row, 0, "TASKS", &header_format)?;
        row += 1;
        worksheet.write_string_with_format(row, 0, "ID", &header_format)?;
        worksheet.write_string_with_format(row, 1, "Name", &header_format)?;
        worksheet.write_string_with_format(row, 2, "Comment", &header_format)?;
        worksheet.write_string_with_format(row, 3, "Completeness", &header_format)?;

        row += 1;
        for task in &report.tasks {
            worksheet.write_number(row, 0, task.id as f64)?;
            worksheet.write_string(row, 1, &task.name)?;
            worksheet.write_string(row, 2, &task.comment)?;
            worksheet.write_string(row, 3, &format!("{}%", task.completeness))?;
            row += 1;
        }

        // Apply auto-sizing for optimal column widths
        worksheet.autofit();

        workbook.save(&self.output_path)?;
        Ok(())
    }

    /// Exports task records to Excel format with formatted table structure.
    ///
    /// This method creates a clean Excel table with task information, suitable
    /// for further analysis or integration with other Excel-based workflows.
    ///
    /// # Arguments
    ///
    /// * `tasks` - The task collection to export
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful Excel generation, or an error if
    /// workbook creation fails.
    fn export_tasks_excel(&self, tasks: &[ExportTask]) -> Result<()> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        let header_format = Format::new().set_bold().set_background_color(rust_xlsxwriter::Color::Gray);

        // Write headers
        worksheet.write_string_with_format(0, 0, "ID", &header_format)?;
        worksheet.write_string_with_format(0, 1, "Name", &header_format)?;
        worksheet.write_string_with_format(0, 2, "Comment", &header_format)?;
        worksheet.write_string_with_format(0, 3, "Completeness", &header_format)?;

        // Write task data
        for (i, task) in tasks.iter().enumerate() {
            let row = i as u32 + 1;
            worksheet.write_number(row, 0, task.id as f64)?;
            worksheet.write_string(row, 1, &task.name)?;
            worksheet.write_string(row, 2, &task.comment)?;
            worksheet.write_string(row, 3, &format!("{}%", task.completeness))?;
        }

        worksheet.autofit();
        workbook.save(&self.output_path)?;
        Ok(())
    }

    /// Exports monthly summary to Excel format with title formatting and statistics.
    ///
    /// This method creates a professional monthly summary report with a formatted
    /// title, daily breakdown table, and summary statistics section.
    ///
    /// # Arguments
    ///
    /// * `summary` - The monthly summary data to export
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful Excel generation, or an error if
    /// workbook creation fails.
    fn export_summary_excel(&self, summary: &ExportSummary) -> Result<()> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        // Create formatting styles
        let header_format = Format::new().set_bold().set_background_color(rust_xlsxwriter::Color::Gray);
        let title_format = Format::new().set_bold().set_font_size(14.0);

        // Write title and daily breakdown
        worksheet.write_string_with_format(0, 0, &format!("Monthly Summary - {}", summary.month), &title_format)?;
        worksheet.write_string_with_format(2, 0, "Date", &header_format)?;
        worksheet.write_string_with_format(2, 1, "Hours", &header_format)?;
        worksheet.write_string_with_format(2, 2, "Type", &header_format)?;

        let mut row = 3;
        for day in &summary.days {
            worksheet.write_string(row, 0, &day.date)?;
            worksheet.write_string(row, 1, &day.hours)?;
            worksheet.write_string(row, 2, if day.is_workday { "Work" } else { "Rest" })?;
            row += 1;
        }

        // Add summary statistics
        row += 1;
        worksheet.write_string(row, 0, "Total Hours")?;
        worksheet.write_string(row, 1, &summary.total_hours)?;
        row += 1;
        worksheet.write_string(row, 0, "Average Hours")?;
        worksheet.write_string(row, 1, &summary.average_hours)?;
        row += 1;
        worksheet.write_string(row, 0, "Total Days")?;
        worksheet.write_number(row, 1, summary.total_days as f64)?;

        worksheet.autofit();
        workbook.save(&self.output_path)?;
        Ok(())
    }

    /// Gathers the data required to render an hourly (SiServer-style) daily report.
    ///
    /// Unlike [`Exporter::gather_report_data`], this method combines both manual
    /// breaks and automatic pauses (respecting the configured minimum pause
    /// duration) so that the resulting hourly grid accurately reflects every
    /// interruption. Tasks are distributed across work intervals using the same
    /// algorithm employed for API submission, ensuring the descriptions match
    /// what is sent to SiServer.
    ///
    /// # Arguments
    ///
    /// * `date` - The target date for which to build the hourly report
    ///
    /// # Returns
    ///
    /// Returns an [`HourlyReport`] describing the workday header and one row per
    /// hour of work, or an error if no workday exists for the date.
    fn gather_hourly_data(&self, date: NaiveDate, locale: &Locale) -> Result<HourlyReport> {
        let workday = Workdays::new()?
            .fetch(date)?
            .ok_or_else(|| msg_error_anyhow!(Message::WorkdayNotFoundForDate(date.to_string())))?;

        // Respect the same interruption sources and thresholds as report submission.
        let config = Config::read()?;
        let monitor_config = config.monitor.as_ref().cloned().unwrap_or_default();
        let breaks = Breaks::new()?.get_daily_breaks(date)?;
        let pauses = Pauses::new()?.set_min_duration(monitor_config.min_pause_duration).get_daily_pauses(date)?;
        let interruptions = report::combine_breaks_and_pauses(&breaks, &pauses);

        let intervals = report::calculate_work_intervals(&workday, &interruptions);
        let tasks = Tasks::new()?.fetch(TaskFilter::Date(date))?;

        // End of the workday (fall back to "now" if the session is still open).
        let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());

        // Assign a human-readable work description to every interval.
        let interval_texts = assign_interval_texts(&tasks, &intervals, locale);

        // Build a chronological timeline of work and break segments covering the day.
        let mut segments: Vec<TimelineSegment> = Vec::new();
        for (i, interval) in intervals.iter().enumerate() {
            segments.push(TimelineSegment {
                start: interval.start,
                end: interval.end,
                text: Some(interval_texts[i].clone()),
            });
        }
        for pause in &interruptions {
            if let Some(pause_end) = pause.end {
                let start = pause.start.max(workday.start);
                let end = pause_end.min(end_time);
                if start < end {
                    segments.push(TimelineSegment { start, end, text: None });
                }
            }
        }
        segments.sort_by_key(|s| s.start);

        // Slice the workday into hour-aligned slots and describe each one.
        let rows = build_hourly_rows(workday.start, end_time, &segments, locale.break_label);

        // Total net worked time is the sum of all work intervals.
        let worked = intervals.iter().fold(Duration::zero(), |acc, i| acc + i.duration);

        // Localized weekday/month names (Monday = index 0, January = index 0).
        let weekday_idx = date.weekday().num_days_from_monday() as usize;
        let month_idx = (date.month().saturating_sub(1)) as usize;

        Ok(HourlyReport {
            date,
            weekday: locale.weekdays[weekday_idx].to_string(),
            month: locale.months[month_idx].to_string(),
            day_hours: worked.num_hours().max(0),
            worked: format_duration(&worked),
            rows,
        })
    }

    /// Renders the hourly daily report to an Excel workbook mirroring the SiServer layout.
    ///
    /// The generated sheet contains a header block (title, date, weekday, workday
    /// length), an hourly table with start/end times and per-hour descriptions,
    /// a total worked-hours row, and an empty comment area.
    ///
    /// # Arguments
    ///
    /// * `date` - The target date for the report
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful workbook creation, or an error if data
    /// gathering or file writing fails.
    fn export_report_excel_hourly(&self, date: NaiveDate) -> Result<()> {
        // Resolve localization and design template from the report config.
        let config = Config::read()?;
        let report_config = config.report.clone().unwrap_or_default();
        let language = Language::from_code(report_config.language.as_deref().unwrap_or("ru"));
        let locale = Locale::for_language(language);
        let template = ReportTemplate::load(report_config.template.as_deref().unwrap_or("siserver"));

        let data = self.gather_hourly_data(date, locale)?;

        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        // Palette parsed from the template (hex → xlsx Color).
        let border_color = template.border();
        let header_fill = template.fill();

        // Builds a base format carrying the given font specification.
        let font_base = |spec: &FontSpec| -> Format {
            let mut fmt = Format::new().set_font_name(spec.name.as_str()).set_font_size(spec.size);
            if spec.bold {
                fmt = fmt.set_bold();
            }
            fmt
        };

        // Title / header-block formats.
        let fmt_title = font_base(&template.fonts.title)
            .set_border(FormatBorder::Thin)
            .set_border_color(border_color)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);
        let fmt_month = font_base(&template.fonts.month).set_align(FormatAlign::Center);
        let fmt_date = font_base(&template.fonts.date)
            .set_border(FormatBorder::Thin)
            .set_border_color(border_color)
            .set_align(FormatAlign::Center);
        let fmt_center = Format::new().set_align(FormatAlign::Center);
        let fmt_right = Format::new().set_align(FormatAlign::Right);

        // Table formats.
        let fmt_header = font_base(&template.fonts.header)
            .set_background_color(header_fill)
            .set_border(FormatBorder::Thin)
            .set_border_color(border_color)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);
        let fmt_time = font_base(&template.fonts.time)
            .set_border(FormatBorder::Thin)
            .set_border_color(border_color)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter);
        let fmt_desc = Format::new()
            .set_border(FormatBorder::Thin)
            .set_border_color(border_color)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_text_wrap();
        let fmt_empty = Format::new()
            .set_border(FormatBorder::Thin)
            .set_border_color(border_color)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_text_wrap();

        // Footer formats.
        let fmt_total_label = Format::new()
            .set_background_color(header_fill)
            .set_border(FormatBorder::Thin)
            .set_border_color(border_color)
            .set_align(FormatAlign::Right);
        let fmt_comment_label = font_base(&template.fonts.header);
        let fmt_comment_box = Format::new()
            .set_border(FormatBorder::Thin)
            .set_border_color(border_color)
            .set_align(FormatAlign::VerticalCenter);

        // Column widths (columns B..F, i.e. 1..5) from the template.
        worksheet.set_column_width(1, template.col_widths[0])?;
        worksheet.set_column_width(2, template.col_widths[1])?;
        worksheet.set_column_width(3, template.col_widths[2])?;
        if template.show_hours_column {
            worksheet.set_column_width(4, template.col_widths[3])?;
        }
        if template.show_result_column {
            worksheet.set_column_width(5, template.col_widths[4])?;
        }

        // Header block.
        worksheet.set_row_height(1, template.title_row_height)?;
        worksheet.merge_range(1, 1, 1, 2, locale.report_title, &fmt_title)?;
        worksheet.write_string_with_format(1, 3, &data.month, &fmt_month)?;
        worksheet.merge_range(2, 1, 2, 2, &data.date.format(locale.date_format).to_string(), &fmt_date)?;

        worksheet.write_string_with_format(4, 1, &data.weekday, &fmt_center)?;
        worksheet.write_string_with_format(4, 2, locale.day_type_working, &fmt_center)?;
        worksheet.write_string_with_format(4, 3, locale.workday_length, &fmt_right)?;
        worksheet.write_number(4, 4, data.day_hours as f64)?;

        // Table header (two rows: day span over start/end, plus per-column headers).
        worksheet.merge_range(6, 1, 6, 2, locale.header_day, &fmt_header)?;
        worksheet.write_string_with_format(7, 1, locale.header_start, &fmt_header)?;
        worksheet.write_string_with_format(7, 2, locale.header_end, &fmt_header)?;
        worksheet.merge_range(6, 3, 7, 3, "", &fmt_header)?;
        if template.show_hours_column {
            worksheet.merge_range(6, 4, 7, 4, locale.header_hours, &fmt_header)?;
        }
        if template.show_result_column {
            worksheet.merge_range(6, 5, 7, 5, locale.header_result, &fmt_header)?;
        }

        // Hourly data rows.
        let mut row: u32 = 8;
        for item in &data.rows {
            worksheet.set_row_height(row, template.data_row_height)?;
            worksheet.write_string_with_format(row, 1, &item.start, &fmt_time)?;
            worksheet.write_string_with_format(row, 2, &item.end, &fmt_time)?;
            worksheet.write_string_with_format(row, 3, &item.description, &fmt_desc)?;
            if template.show_hours_column {
                worksheet.write_string_with_format(row, 4, "", &fmt_empty)?;
            }
            if template.show_result_column {
                worksheet.write_string_with_format(row, 5, "", &fmt_empty)?;
            }
            row += 1;
        }
        let last_data_row = row.saturating_sub(1);

        // Total worked-hours row (two blank rows below the table, as in the template).
        let total_row = last_data_row + 3;
        worksheet.merge_range(total_row, 1, total_row, 3, locale.total_worked, &fmt_total_label)?;
        worksheet.write_string_with_format(total_row, 4, &data.worked, &fmt_time)?;

        // Comment label and empty comment box.
        if template.show_comment {
            let comment_row = total_row + 2;
            worksheet.write_string_with_format(comment_row, 1, locale.comment, &fmt_comment_label)?;
            let box_top = comment_row + 1;
            let box_bottom = box_top + template.comment_rows.saturating_sub(1);
            worksheet.merge_range(box_top, 1, box_bottom, 5, "", &fmt_comment_box)?;
        }

        workbook.save(&self.output_path)?;
        Ok(())
    }
}

/// A single work or break segment on the daily timeline.
///
/// Segments are used as an intermediate representation between raw work
/// intervals/pauses and the final hour-by-hour grid. A segment with `text`
/// set to `Some` represents work; `None` represents a break/pause.
struct TimelineSegment {
    /// Segment start timestamp.
    start: NaiveDateTime,
    /// Segment end timestamp.
    end: NaiveDateTime,
    /// Work description, or `None` for a break/pause.
    text: Option<String>,
}

/// A single rendered row of the hourly report (one hour of the workday).
struct HourlyRow {
    /// Hour slot start in "HH:MM" format.
    start: String,
    /// Hour slot end in "HH:MM" format.
    end: String,
    /// Description of what happened during the hour ("Перерыв" for breaks).
    description: String,
}

/// Aggregated data required to render an hourly daily report.
struct HourlyReport {
    /// Report date.
    date: NaiveDate,
    /// Localized weekday name.
    weekday: String,
    /// Localized month name.
    month: String,
    /// Whole worked hours, used for the "workday length" header cell.
    day_hours: i64,
    /// Total net worked time formatted as "HH:MM".
    worked: String,
    /// Hour-by-hour rows.
    rows: Vec<HourlyRow>,
}

/// Distributes tasks across work intervals and returns a description per interval.
///
/// This mirrors the distribution logic used when submitting reports to the API:
/// when there are more tasks than intervals, multiple tasks are combined into a
/// single interval; otherwise a task may span several consecutive intervals.
/// When there are no tasks, the locale's generic work label is used.
fn assign_interval_texts(tasks: &[Task], intervals: &[WorkInterval], locale: &Locale) -> Vec<String> {
    let num_intervals = intervals.len();
    let num_tasks = tasks.len();
    let mut texts = vec![locale.work_generic.to_string(); num_intervals];

    if num_intervals == 0 || num_tasks == 0 {
        return texts;
    }

    if num_tasks >= num_intervals {
        let base = num_tasks / num_intervals;
        let mut extra = num_tasks % num_intervals;
        let mut iter = tasks.iter();

        for slot in texts.iter_mut() {
            let count = base + if extra > 0 { 1 } else { 0 };
            if extra > 0 {
                extra -= 1;
            }
            let parts: Vec<String> = iter.by_ref().take(count).map(|t| locale.work_text(&t.name)).collect();
            if !parts.is_empty() {
                *slot = parts.join(". ");
            }
        }
    } else {
        let base = num_intervals / num_tasks;
        let mut extra = num_intervals % num_tasks;
        let mut idx = 0;

        for task in tasks {
            let count = base + if extra > 0 { 1 } else { 0 };
            if extra > 0 {
                extra -= 1;
            }
            let text = locale.work_text(&task.name);
            for _ in 0..count {
                if idx < num_intervals {
                    texts[idx] = text.clone();
                    idx += 1;
                }
            }
        }
    }

    texts
}

/// Truncates a timestamp down to the start of its hour (zeroing minutes/seconds).
fn floor_to_hour(dt: NaiveDateTime) -> NaiveDateTime {
    dt.with_minute(0).and_then(|d| d.with_second(0)).and_then(|d| d.with_nanosecond(0)).unwrap_or(dt)
}

/// Removes duplicate description parts anywhere in the slot, preserving order.
///
/// Unlike a consecutive-only dedup, this collapses identical entries even when
/// they are separated by other parts. For example:
/// - "Работа по задаче X. Перерыв. Работа по задаче X" → "Работа по задаче X. Перерыв"
/// - "Перерыв. Работа по задаче X. Перерыв" → "Перерыв. Работа по задаче X"
///
/// The first occurrence of each unique part is kept in its original position.
fn dedup_preserve_order(parts: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    parts.retain(|part| seen.insert(part.clone()));
}

/// Slices the workday into hour-aligned slots and describes each slot.
///
/// The grid starts at the top of the hour containing `work_start` and advances
/// in one-hour steps until `work_end`. For each slot the overlapping timeline
/// segments are collected in chronological order; work segments contribute their
/// task description and break segments contribute `break_label`. Slots that
/// contain no work at all are labelled with `break_label`.
fn build_hourly_rows(work_start: NaiveDateTime, work_end: NaiveDateTime, segments: &[TimelineSegment], break_label: &str) -> Vec<HourlyRow> {
    let mut rows = Vec::new();
    if work_end <= work_start {
        return rows;
    }

    let mut slot_start = floor_to_hour(work_start);
    while slot_start < work_end {
        let slot_grid_end = slot_start + Duration::hours(1);
        let slot_end = slot_grid_end.min(work_end);

        // Only consider the portion of the slot within the actual workday.
        let window_start = slot_start.max(work_start);

        let mut parts: Vec<String> = Vec::new();
        for segment in segments {
            let overlap_start = segment.start.max(window_start);
            let overlap_end = segment.end.min(slot_end);
            if overlap_start < overlap_end {
                match &segment.text {
                    Some(text) => parts.push(text.clone()),
                    None => parts.push(break_label.to_string()),
                }
            }
        }
        dedup_preserve_order(&mut parts);

        let description = if parts.is_empty() { break_label.to_string() } else { parts.join(". ") };

        rows.push(HourlyRow {
            start: slot_start.format("%H:%M").to_string(),
            end: slot_end.format("%H:%M").to_string(),
            description,
        });

        slot_start = slot_grid_end;
    }

    rows
}
