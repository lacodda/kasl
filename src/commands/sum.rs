//! Monthly working hours summary command.
//!
//! Generates comprehensive monthly reports showing daily work hours, productivity metrics, and calendar integration with company rest days.
//!
//! ## Usage
//!
//! ```bash
//! # Generate current month summary
//! kasl sum
//!
//! # The same month as JSON
//! kasl sum --json
//! ```
//!
//! The month is sent with `kasl report --month`. `sum` had a `--send` flag
//! until 1.21 that was parsed and never read; it is gone rather than wired
//! up, so there is one command that sends a month.

use crate::{
    libs::{
        messages::Message,
        output::{self, Format},
        summary::{Summary, SummaryFormatter},
        view::View,
    },
    msg_print,
};
use anyhow::Result;
use chrono::Local;
use clap::Args;

/// Command-line arguments for the monthly summary command.
#[derive(Debug, Args)]
pub struct SumArgs {
    #[command(flatten)]
    format: Format,
}

/// Generates and displays a comprehensive monthly working hours summary.
///
/// Creates a detailed analysis of work patterns for the current month, including
/// productivity calculations, rest day integration, and daily breakdowns.
///
pub async fn cmd(args: SumArgs) -> Result<()> {
    let now = Local::now();
    let month = Summary::month(now.date_naive()).await?;
    if args.format.json {
        return output::print_json(&month.to_month_json());
    }

    msg_print!(Message::WorkingHoursForMonth(now.format("%B, %Y").to_string()), true);
    let event_summary = (month.days, month.total, month.average).format_summary();
    View::sum(&event_summary)?;
    if let Some(average_productivity) = month.productivity {
        msg_print!(Message::MonthlyProductivity(average_productivity), true);
    }

    Ok(())
}
