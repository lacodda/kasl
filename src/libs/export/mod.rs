//! Exports reports, tasks and summaries to CSV, JSON and Excel,
//! including the hourly (SiServer-style) Excel layout.
//!
//! ```rust,no_run
//! # async fn f() -> anyhow::Result<()> {
//! use kasl::libs::export::{Exporter, ExportFormat, ExportData};
//! use chrono::NaiveDate;
//!
//! let exporter = Exporter::new(ExportFormat::Csv, None);
//! exporter.export(ExportData::Report, NaiveDate::from_ymd_opt(2025, 1, 15).unwrap()).await?;
//! # Ok(())
//! # }
//! ```

use crate::{
    db::{pauses::Pauses, tasks::Tasks, workdays::Workdays},
    libs::{
        config::Config,
        formatter::format_duration,
        locale::{Language, Locale},
        messages::Message,
        report,
        report_template::{FontSpec, ReportTemplate},
        task::TaskFilter,
    },
    msg_error_anyhow, msg_info, msg_success,
};
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate};
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

mod hourly;
use hourly::{HourlyReport, assign_tasks_to_hour_slots, build_hourly_rows, classify_hour_slots};

/// Output formats for exports.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportFormat {
    Csv,
    /// Pretty-printed JSON.
    Json,
    /// One worksheet per export, headers and autofit applied.
    Excel,
}

/// What gets exported.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportData {
    /// The daily report: intervals, tasks, productivity.
    Report,
    /// The date's tasks.
    Tasks,
    /// The month's totals and per-day hours.
    Summary,
    /// Report + tasks + summary: one JSON file, or suffixed files for CSV/Excel.
    All,
}

/// A daily report as exported; fields are pre-formatted strings.
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

/// One work interval row in the exported report.
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

/// One task row in the exported report.
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

/// A monthly summary as exported.
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

/// One day's line in the exported monthly summary.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportDaySum {
    /// Date in YYYY-MM-DD format
    pub date: String,
    /// Working hours for this date formatted as duration
    pub hours: String,
    /// Whether this was a working day (true) or rest day (false)
    pub is_workday: bool,
}

/// Gathers data and writes it in the chosen format.
pub struct Exporter {
    format: ExportFormat,
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
    /// Builds an exporter; without a path the file is named
    /// `kasl_export_{YYYYMMDD_HHMMSS}.{ext}` in the current directory.
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

    /// Toggles the hourly (SiServer-style) layout; only Excel report
    /// exports honor it.
    pub fn hourly(mut self, hourly: bool) -> Self {
        self.hourly = hourly;
        self
    }

    /// Runs the export for the requested data type.
    ///
    /// ```rust,no_run
    /// # async fn f() -> anyhow::Result<()> {
    /// use kasl::libs::export::{Exporter, ExportFormat, ExportData};
    /// use chrono::NaiveDate;
    ///
    /// let exporter = Exporter::new(ExportFormat::Json, None);
    /// let date = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
    /// exporter.export(ExportData::Report, date).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn export(&self, data_type: ExportData, date: NaiveDate) -> Result<()> {
        match data_type {
            ExportData::Report => self.export_report(date).await,
            ExportData::Tasks => self.export_tasks(date).await,
            ExportData::Summary => self.export_summary(date).await,
            ExportData::All => self.export_all(date).await,
        }
    }

    /// Exports the daily report (hourly Excel layout when requested).
    async fn export_report(&self, date: NaiveDate) -> Result<()> {
        // Hourly (SiServer-style) layout is only meaningful for Excel output.
        // When requested, delegate to the dedicated renderer and skip the
        // generic report layout entirely.
        if self.hourly
            && let ExportFormat::Excel = self.format
        {
            self.export_report_excel_hourly(date)?;
            msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
            return Ok(());
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

    /// Exports the date's tasks.
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

    /// Exports the monthly summary.
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

    /// Exports everything: one nested JSON file, or three suffixed files
    /// (`_report`, `_tasks`, `_summary`) for CSV/Excel.
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

    /// Assembles the daily report from workday, tasks and pauses.
    ///
    /// The productivity here is the simplified `net/gross` ratio, not the
    /// full [`crate::libs::productivity::Productivity`] calculation - an
    /// export must not differ depending on which thresholds are configured.
    fn gather_report_data(&self, date: NaiveDate) -> Result<ExportReport> {
        // Retrieve the primary workday record or fail if none exists
        let workday = Workdays::new()?
            .fetch(date)?
            .ok_or_else(|| msg_error_anyhow!(Message::WorkdayNotFoundForDate(date.to_string())))?;

        // Collect associated tasks and pause data
        let tasks = Tasks::new()?.fetch(TaskFilter::Date(date))?;
        let pauses = Pauses::new()?.get_workday_pauses(&workday)?;

        // Determine end time (use current time if workday is still active)
        let end_time = report::workday_end_time(&workday, &pauses);

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

    /// Aggregates the month's workdays into totals and per-day rows.
    fn gather_summary_data(&self, date: NaiveDate) -> Result<ExportSummary> {
        // Retrieve all workday records for the month containing the specified date
        let workdays = Workdays::new()?.fetch_month(date)?;

        // Initialize aggregation variables
        let mut days = Vec::new();
        let mut total_duration = Duration::zero();

        // Process each workday to calculate duration and accumulate statistics
        for workday in &workdays {
            // Determine end time (now while the day is still today, otherwise
            // the last observed activity - see report::workday_end_time).
            let day_pauses = Pauses::new()?.get_workday_pauses(workday)?;
            let end_time = report::workday_end_time(workday, &day_pauses);
            let duration = end_time - workday.start;
            total_duration += duration;

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

    /// Writes the report as three labelled CSV sections (intervals,
    /// summary, tasks) separated by blank rows.
    fn export_report_csv(&self, report: &ExportReport) -> Result<()> {
        let mut wtr = csv::Writer::from_path(&self.output_path)?;

        // Write work intervals section with headers
        wtr.write_record(["WORK INTERVALS", "", "", ""])?;
        wtr.write_record(["Index", "Start", "End", "Duration"])?;
        for interval in &report.intervals {
            wtr.write_record(&[
                interval.index.to_string(),
                interval.start.clone(),
                interval.end.clone(),
                interval.duration.clone(),
            ])?;
        }

        // Add spacing and summary section
        wtr.write_record(["", "", "", ""])?;
        wtr.write_record(["SUMMARY", "", "", ""])?;
        wtr.write_record(["Date", &report.date, "", ""])?;
        wtr.write_record(["Total Hours", &report.total_hours, "", ""])?;
        wtr.write_record(["Productivity", &format!("{:.1}%", report.productivity), "", ""])?;

        // Add spacing and tasks section
        wtr.write_record(["", "", "", ""])?;
        wtr.write_record(["TASKS", "", "", ""])?;
        wtr.write_record(["ID", "Name", "Comment", "Completeness"])?;
        for task in &report.tasks {
            wtr.write_record(&[task.id.to_string(), task.name.clone(), task.comment.clone(), format!("{}%", task.completeness)])?;
        }

        wtr.flush()?;
        Ok(())
    }

    fn export_tasks_csv(&self, tasks: &[ExportTask]) -> Result<()> {
        let mut wtr = csv::Writer::from_path(&self.output_path)?;
        wtr.write_record(["ID", "Name", "Comment", "Completeness"])?;

        for task in tasks {
            wtr.write_record(&[task.id.to_string(), task.name.clone(), task.comment.clone(), format!("{}%", task.completeness)])?;
        }

        wtr.flush()?;
        Ok(())
    }

    fn export_summary_csv(&self, summary: &ExportSummary) -> Result<()> {
        let mut wtr = csv::Writer::from_path(&self.output_path)?;

        // Write title and daily breakdown
        wtr.write_record(&[format!("Monthly Summary - {}", summary.month), "".to_owned(), "".to_owned()])?;
        wtr.write_record(["Date", "Hours", "Type"])?;

        for day in &summary.days {
            wtr.write_record(&[
                day.date.clone(),
                day.hours.clone(),
                if day.is_workday { "Work".to_owned() } else { "Rest".to_owned() },
            ])?;
        }

        // Add summary statistics
        wtr.write_record(["", "", ""])?;
        wtr.write_record(["Total Hours", &summary.total_hours, ""])?;
        wtr.write_record(["Average Hours", &summary.average_hours, ""])?;
        wtr.write_record(["Total Days", &summary.total_days.to_string(), ""])?;

        wtr.flush()?;
        Ok(())
    }

    fn export_report_json(&self, report: &ExportReport) -> Result<()> {
        let json = serde_json::to_string_pretty(report)?;
        File::create(&self.output_path)?.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Writes the report worksheet: the same three sections as the CSV.
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
        worksheet.write_string(row, 1, format!("{:.1}%", report.productivity))?;

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
            worksheet.write_string(row, 3, format!("{}%", task.completeness))?;
            row += 1;
        }

        // Apply auto-sizing for optimal column widths
        worksheet.autofit();

        workbook.save(&self.output_path)?;
        Ok(())
    }

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
            worksheet.write_string(row, 3, format!("{}%", task.completeness))?;
        }

        worksheet.autofit();
        workbook.save(&self.output_path)?;
        Ok(())
    }

    fn export_summary_excel(&self, summary: &ExportSummary) -> Result<()> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        // Create formatting styles
        let header_format = Format::new().set_bold().set_background_color(rust_xlsxwriter::Color::Gray);
        let title_format = Format::new().set_bold().set_font_size(14.0);

        // Write title and daily breakdown
        worksheet.write_string_with_format(0, 0, format!("Monthly Summary - {}", summary.month), &title_format)?;
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
    /// interruption. Tasks are distributed one-per-hour across work hour slots
    /// (not across work intervals): fewer tasks span contiguous hour blocks;
    /// surplus tasks are appended only to hours without a break.
    ///
    fn gather_hourly_data(&self, date: NaiveDate, locale: &Locale) -> Result<HourlyReport> {
        let workday = Workdays::new()?
            .fetch(date)?
            .ok_or_else(|| msg_error_anyhow!(Message::WorkdayNotFoundForDate(date.to_string())))?;

        // Respect the same interruption sources and thresholds as report submission.
        let config = Config::read()?;
        let monitor_config = config.monitor.as_ref().cloned().unwrap_or_default();
        let pauses = Pauses::new()?
            .set_min_duration(monitor_config.min_pause_duration)
            .get_workday_pauses(&workday)?;

        let intervals = report::calculate_work_intervals(&workday, &pauses);
        let tasks = Tasks::new()?.fetch(TaskFilter::Date(date))?;

        // End of the workday (now while it is still today, otherwise the last
        // observed activity - see report::workday_end_time).
        let end_time = report::workday_end_time(&workday, &pauses);

        let slots = classify_hour_slots(workday.start, end_time, &intervals, &pauses);
        let task_texts = assign_tasks_to_hour_slots(&tasks, &slots, locale);
        let rows = build_hourly_rows(&slots, &task_texts, locale.break_label);

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
    fn export_report_excel_hourly(&self, date: NaiveDate) -> Result<()> {
        // Resolve localization and design template from the report config.
        let config = Config::read()?;
        let report_config = config.report.clone().unwrap_or_default();
        // English unless the config asks otherwise; `from_code` defaults to it too.
        let language = Language::from_code(report_config.language.as_deref().unwrap_or("en"));
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
