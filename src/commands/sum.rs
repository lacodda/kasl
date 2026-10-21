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
//! # Submit monthly report to external API
//! kasl sum --send
//! ```

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
///
/// Currently supports basic summary generation with optional report submission.
/// Future versions may add date range selection and detailed filtering options.
#[derive(Debug, Args)]
pub struct SumArgs {
    /// Submit the monthly summary report
    ///
    /// When specified, the generated summary will be submitted to the configured
    /// reporting API in addition to being displayed locally. This is useful for
    /// organizational reporting requirements.
    #[arg(long, help = "Send report", conflicts_with = "json")]
    send: bool,

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
