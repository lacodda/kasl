use crate::{
    db::{pauses::Pauses, tasks::Tasks, workdays::Workdays},
    libs::{formatter::format_duration, messages::Message, report, task::TaskFilter},
    msg_error_anyhow, msg_info, msg_success,
};
use anyhow::Result;
use chrono::{Duration, Local, NaiveDate};
use rust_xlsxwriter::{Format, Workbook};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

/// Supported export formats
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportFormat {
    Csv,
    Json,
    Excel,
}

/// Data to export
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportData {
    /// Export daily report
    Report,
    /// Export tasks
    Tasks,
    /// Export monthly summary
    Summary,
    /// Export all data
    All,
}

/// Serializable report data
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportReport {
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub total_hours: String,
    pub productivity: f64,
    pub intervals: Vec<ExportInterval>,
    pub tasks: Vec<ExportTask>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportInterval {
    pub index: usize,
    pub start: String,
    pub end: String,
    pub duration: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportTask {
    pub id: i32,
    pub name: String,
    pub comment: String,
    pub completeness: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportSummary {
    pub month: String,
    pub days: Vec<ExportDaySum>,
    pub total_hours: String,
    pub average_hours: String,
    pub total_days: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportDaySum {
    pub date: String,
    pub hours: String,
    pub is_workday: bool,
}

/// Main export handler
pub struct Exporter {
    format: ExportFormat,
    output_path: PathBuf,
}

impl Exporter {
    pub fn new(format: ExportFormat, output_path: Option<PathBuf>) -> Self {
        let default_name = format!("kasl_export_{}", Local::now().format("%Y%m%d_%H%M%S"));
        let extension = match format {
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
            ExportFormat::Excel => "xlsx",
        };

        let output_path = output_path.unwrap_or_else(|| PathBuf::from(format!("{}.{}", default_name, extension)));

        Self { format, output_path }
    }

    /// Export data based on type
    pub async fn export(&self, data_type: ExportData, date: NaiveDate) -> Result<()> {
        match data_type {
            ExportData::Report => self.export_report(date).await,
            ExportData::Tasks => self.export_tasks(date).await,
            ExportData::Summary => self.export_summary(date).await,
            ExportData::All => self.export_all(date).await,
        }
    }

    /// Export daily report
    async fn export_report(&self, date: NaiveDate) -> Result<()> {
        let report_data = self.gather_report_data(date)?;

        match self.format {
            ExportFormat::Csv => self.export_report_csv(&report_data)?,
            ExportFormat::Json => self.export_report_json(&report_data)?,
            ExportFormat::Excel => self.export_report_excel(&report_data)?,
        }

        msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
        Ok(())
    }

    /// Export tasks
    async fn export_tasks(&self, date: NaiveDate) -> Result<()> {
        let tasks = Tasks::new()?.fetch(TaskFilter::Date(date))?;
        let export_tasks: Vec<ExportTask> = tasks
            .into_iter()
            .map(|t| ExportTask {
                id: t.id.unwrap_or(0),
                name: t.name,
                comment: t.comment,
                completeness: t.completeness.unwrap_or(100),
            })
            .collect();

        match self.format {
            ExportFormat::Csv => self.export_tasks_csv(&export_tasks)?,
            ExportFormat::Json => {
                let json = serde_json::to_string_pretty(&export_tasks)?;
                File::create(&self.output_path)?.write_all(json.as_bytes())?;
            }
            ExportFormat::Excel => self.export_tasks_excel(&export_tasks)?,
        }

        msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
        Ok(())
    }

    /// Export monthly summary
    async fn export_summary(&self, date: NaiveDate) -> Result<()> {
        let summary_data = self.gather_summary_data(date)?;

        match self.format {
            ExportFormat::Csv => self.export_summary_csv(&summary_data)?,
            ExportFormat::Json => {
                let json = serde_json::to_string_pretty(&summary_data)?;
                File::create(&self.output_path)?.write_all(json.as_bytes())?;
            }
            ExportFormat::Excel => self.export_summary_excel(&summary_data)?,
        }

        msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
        Ok(())
    }

    /// Export all data
    async fn export_all(&self, date: NaiveDate) -> Result<()> {
        msg_info!(Message::ExportingAllData);

        // For JSON, we can combine all data
        if let ExportFormat::Json = self.format {
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

            let all_data = serde_json::json!({
                "export_date": Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                "daily_report": report,
                "tasks": tasks,
                "monthly_summary": summary,
            });

            let json = serde_json::to_string_pretty(&all_data)?;
            File::create(&self.output_path)?.write_all(json.as_bytes())?;
        } else {
            // For other formats, create separate files
            let base = self.output_path.file_stem().unwrap().to_string_lossy();
            let ext = self.output_path.extension().unwrap().to_string_lossy();

            // Export each type with suffix
            let report_path = self.output_path.with_file_name(format!("{}_report.{}", base, ext));
            let tasks_path = self.output_path.with_file_name(format!("{}_tasks.{}", base, ext));
            let summary_path = self.output_path.with_file_name(format!("{}_summary.{}", base, ext));

            let report_exporter = Exporter::new(self.format, Some(report_path));
            let tasks_exporter = Exporter::new(self.format, Some(tasks_path));
            let summary_exporter = Exporter::new(self.format, Some(summary_path));

            report_exporter.export_report(date).await?;
            tasks_exporter.export_tasks(date).await?;
            summary_exporter.export_summary(date).await?;

            return Ok(());
        }

        msg_success!(Message::ExportCompleted(self.output_path.display().to_string()));
        Ok(())
    }

    /// Gather report data
    fn gather_report_data(&self, date: NaiveDate) -> Result<ExportReport> {
        let workday = Workdays::new()?
            .fetch(date)?
            .ok_or_else(|| msg_error_anyhow!(Message::WorkdayNotFoundForDate(date.to_string())))?;

        let tasks = Tasks::new()?.fetch(TaskFilter::Date(date))?;
        let pauses = Pauses::new()?.fetch(date, 0)?;

        let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
        let intervals = report::calculate_work_intervals(&workday, &pauses);

        let total_pause_duration = pauses.iter().filter_map(|p| p.duration).fold(Duration::zero(), |acc, d| acc + d);

        let gross_duration = end_time - workday.start;
        let net_duration = gross_duration - total_pause_duration;
        let productivity = (net_duration.num_seconds() as f64 / gross_duration.num_seconds() as f64) * 100.0;

        Ok(ExportReport {
            date: date.format("%Y-%m-%d").to_string(),
            start_time: workday.start.format("%H:%M").to_string(),
            end_time: end_time.format("%H:%M").to_string(),
            total_hours: format_duration(&net_duration),
            productivity: (productivity * 10.0).round() / 10.0,
            intervals: intervals
                .iter()
                .enumerate()
                .map(|(i, interval)| ExportInterval {
                    index: i + 1,
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

    /// Gather summary data for the month
    fn gather_summary_data(&self, date: NaiveDate) -> Result<ExportSummary> {
        let workdays = Workdays::new()?.fetch_month(date)?;
        let mut days = Vec::new();
        let mut total_duration = Duration::zero();

        for workday in &workdays {
            let end_time = workday.end.unwrap_or_else(|| Local::now().naive_local());
            let duration = end_time - workday.start;
            total_duration = total_duration + duration;

            days.push(ExportDaySum {
                date: workday.date.format("%Y-%m-%d").to_string(),
                hours: format_duration(&duration),
                is_workday: true,
            });
        }

        let avg_duration = if !workdays.is_empty() {
            Duration::seconds(total_duration.num_seconds() / workdays.len() as i64)
        } else {
            Duration::zero()
        };

        Ok(ExportSummary {
            month: date.format("%B %Y").to_string(),
            days,
            total_hours: format_duration(&total_duration),
            average_hours: format_duration(&avg_duration),
            total_days: workdays.len(),
        })
    }

    // CSV export methods
    fn export_report_csv(&self, report: &ExportReport) -> Result<()> {
        let mut wtr = csv::Writer::from_path(&self.output_path)?;

        // Write intervals
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

        wtr.write_record(&["", "", "", ""])?;
        wtr.write_record(&["SUMMARY", "", "", ""])?;
        wtr.write_record(&["Date", &report.date.clone(), "", ""])?;
        wtr.write_record(&["Total Hours", &report.total_hours.clone(), "", ""])?;
        wtr.write_record(&["Productivity", &format!("{:.1}%", report.productivity), "", ""])?;

        wtr.write_record(&["", "", "", ""])?;
        wtr.write_record(&["TASKS", "", "", ""])?;
        wtr.write_record(&["ID", "Name", "Comment", "Completeness"])?;
        for task in &report.tasks {
            wtr.write_record(&[task.id.to_string(), task.name.clone(), task.comment.clone(), format!("{}%", task.completeness)])?;
        }

        wtr.flush()?;
        Ok(())
    }

    fn export_tasks_csv(&self, tasks: &[ExportTask]) -> Result<()> {
        let mut wtr = csv::Writer::from_path(&self.output_path)?;
        wtr.write_record(&["ID", "Name", "Comment", "Completeness"])?;

        for task in tasks {
            wtr.write_record(&[task.id.to_string(), task.name.clone(), task.comment.clone(), format!("{}%", task.completeness)])?;
        }

        wtr.flush()?;
        Ok(())
    }

    fn export_summary_csv(&self, summary: &ExportSummary) -> Result<()> {
        let mut wtr = csv::Writer::from_path(&self.output_path)?;

        wtr.write_record(&[format!("Monthly Summary - {}", summary.month), "".to_owned(), "".to_owned()])?;
        wtr.write_record(&["Date", "Hours", "Type"])?;

        for day in &summary.days {
            wtr.write_record(&[
                day.date.clone(),
                day.hours.clone(),
                if day.is_workday { "Work".to_string().to_owned() } else { "Rest".to_owned() },
            ])?;
        }

        wtr.write_record(&["", "", ""])?;
        wtr.write_record(&["Total Hours", &summary.total_hours.clone(), ""])?;
        wtr.write_record(&["Average Hours", &summary.average_hours.clone(), ""])?;
        wtr.write_record(&["Total Days", &summary.total_days.to_string(), ""])?;

        wtr.flush()?;
        Ok(())
    }

    // JSON export is handled inline
    fn export_report_json(&self, report: &ExportReport) -> Result<()> {
        let json = serde_json::to_string_pretty(report)?;
        File::create(&self.output_path)?.write_all(json.as_bytes())?;
        Ok(())
    }

    // Excel export methods using rust_xlsxwriter
    fn export_report_excel(&self, report: &ExportReport) -> Result<()> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        // Create formats
        let header_format = Format::new().set_bold().set_background_color(rust_xlsxwriter::Color::Gray);

        // Write intervals
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

        // Auto-fit columns
        worksheet.autofit();

        workbook.save(&self.output_path)?;
        Ok(())
    }

    fn export_tasks_excel(&self, tasks: &[ExportTask]) -> Result<()> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        let header_format = Format::new().set_bold().set_background_color(rust_xlsxwriter::Color::Gray);

        worksheet.write_string_with_format(0, 0, "ID", &header_format)?;
        worksheet.write_string_with_format(0, 1, "Name", &header_format)?;
        worksheet.write_string_with_format(0, 2, "Comment", &header_format)?;
        worksheet.write_string_with_format(0, 3, "Completeness", &header_format)?;

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

    fn export_summary_excel(&self, summary: &ExportSummary) -> Result<()> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        let header_format = Format::new().set_bold().set_background_color(rust_xlsxwriter::Color::Gray);

        let title_format = Format::new().set_bold().set_font_size(14.0);

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
}
