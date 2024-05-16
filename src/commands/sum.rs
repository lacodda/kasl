use crate::{
    db::events::{Events, SelectRequest},
    libs::{
        event::{FormatEventGroup, MergeEventGroup, MergeEvents},
        view::View,
    },
};
use chrono::Local;
use clap::Args;
use std::error::Error;

#[derive(Debug, Args)]
pub struct SumArgs {
    #[arg(long, help = "Send report")]
    send: bool,
}

pub fn cmd(_sum_args: SumArgs) -> Result<(), Box<dyn Error>> {
    let now = Local::now();
    println!("\nWorking hours for {}", now.format("%B, %Y"));
    let event_summary = Events::new()?.fetch(SelectRequest::Monthly)?.summary().calc().format();

    View::sum(&event_summary)?;

    Ok(())
}
