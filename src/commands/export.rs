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

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// What data to export
    #[arg(value_enum, default_value = "report")]
    data: ExportData,

    /// Export format
    #[arg(short, long, value_enum, default_value = "csv")]
    format: ExportFormat,

    /// Output file path
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Date to export data for (YYYY-MM-DD or 'today')
    #[arg(short, long, default_value = "today")]
    date: String,
}

pub async fn cmd(args: ExportArgs) -> Result<()> {
    let date = parse_date(&args.date)?;

    msg_info!(Message::ExportingData(format!("{:?}", args.data), format!("{:?}", args.format)));

    let exporter = Exporter::new(args.format, args.output);
    exporter.export(args.data, date).await?;

    Ok(())
}

fn parse_date(date_str: &str) -> Result<NaiveDate> {
    if date_str.to_lowercase() == "today" {
        Ok(Local::now().date_naive())
    } else {
        Ok(NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?)
    }
}
